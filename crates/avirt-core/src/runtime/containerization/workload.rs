//! The Containerization runtime, where the `avirtvmd` helper hosts each workload container's VM and the mediator stays a host process.

#[cfg(test)]
mod tests;

use super::cache::{Artifacts, AvirtvmdPreparer, PINNED};
use super::images;
use super::intent::{Intent, State};
use super::lifecycle::{ChildSupervisor, HelperHandle, HelperSpec, HelperSupervisor as _, Incarnation};
use super::proto::{Event, EventKind, RequestOp, ResponseResult, StartWorkload};
use super::reconcile::reap;
use super::request::{Inputs, build};
use super::temp::sweep_stale;
use crate::egress::Proxy;
use crate::runtime::{
    Exit, MediatorSpec, Runtime, Stopped, Swept, WorkloadExit, WorkloadHandle, WorkloadSpec, spawn_mediator,
};
use crate::state::{self, remove_tree};
use alloc::sync::Arc;
use anyhow::{Context as _, Result, anyhow, bail};
use async_trait::async_trait;
use core::time::Duration;
use std::io;
use std::path::{Path, PathBuf};
use tokio::task;
use tokio::time;

/// How long `release` lets a helper finish stopping before it gives up on removing its state.
const RELEASE_GRACE: Duration = Duration::from_secs(5);
/// How long the helper has to answer a start request. The request returns when the VM runs.
const START_TIMEOUT: Duration = Duration::from_mins(2);
/// How long `stop` waits for the exit event that follows a stop.
const EXIT_EVENT_WAIT: Duration = Duration::from_millis(200);

/// The runtime that hosts each workload's VM in an `avirtvmd` helper.
///
/// `open` imports the image archive, prepares the cached kernel, init filesystem, and root filesystem, and writes the launch intent. The handle it returns starts the mediator, then the helper, and `wait` returns when the container or the mediator exits. It runs without the `container` command.
#[derive(Debug)]
pub struct Containerization {
    /// The kernel, init filesystem, and root filesystem caches.
    artifacts: Artifacts,
    /// `containers/`, where each helper keeps its workload's VM state.
    containers: PathBuf,
    /// The directory of OCI archives that `mise run build-image` writes.
    image_archives: PathBuf,
    /// `images/`, where verified image blobs live.
    images: PathBuf,
    /// Runs the helpers as child processes.
    supervisor: Arc<ChildSupervisor>,
    /// The signed avirtvmd binary.
    vmd_bin: PathBuf,
}

/// One workload under [`Containerization`].
struct Handle {
    /// The control connection, once the helper runs.
    client: Option<HelperHandle>,
    /// `containers/`.
    containers: PathBuf,
    /// The incarnation that the helper starts with.
    incarnation: Incarnation,
    /// The workload's mediator, once started and until `stop_mediator`.
    proxy: Option<Proxy>,
    /// The request that starts the workload's VM.
    request: StartWorkload,
    /// The workload to run.
    spec: WorkloadSpec,
    /// How long the helper has to answer the start request.
    start_timeout: Duration,
    /// Runs the helper.
    supervisor: Arc<ChildSupervisor>,
    /// The signed avirtvmd binary.
    vmd_bin: PathBuf,
}

impl Containerization {
    /// A runtime over the state root `state`, with the helper at `vmd_bin` and the image archives
    /// in `image_archives`.
    #[must_use]
    pub fn new(state: &state::Root, vmd_bin: PathBuf, image_archives: PathBuf) -> Self {
        let preparer = Arc::new(AvirtvmdPreparer::new(vmd_bin.clone()));
        Self {
            artifacts: Artifacts::new(state.cache(), preparer, &PINNED),
            containers: state.containers(),
            image_archives,
            images: state.images(),
            supervisor: Arc::new(ChildSupervisor::new(state.workloads())),
            vmd_bin,
        }
    }
}

#[async_trait]
impl Runtime for Containerization {
    async fn open(&self, spec: WorkloadSpec) -> Result<Box<dyn WorkloadHandle>> {
        let archive = self.image_archives.join(format!("{}.tar", spec.image.replace([':', '/'], "_")));
        let imported = images::import(&archive, &self.images).await?;
        let kernel = self.artifacts.kernel().await?;
        let initfs = self.artifacts.initfs().await?;
        let rootfs = self.artifacts.rootfs(&imported.manifest, &imported.layers).await?;
        let container_dir = self.containers.join(&spec.id);
        let stdio_log = spec.logs_dir.join(format!("{}.stdio.ndjson", spec.id));
        let request = build(
            &spec,
            &Inputs {
                container_dir: &container_dir,
                image_config: &imported.config,
                initfs: &initfs,
                kernel: &kernel,
                rootfs_base: &rootfs,
                stdio_log: &stdio_log,
            },
        )?;
        let incarnation = Incarnation::random();
        let intent = Intent {
            incarnation: incarnation.to_hex(),
            pid: None,
            spec_digest: request.spec_digest.clone(),
            state: State::Intended,
            workload: spec.id.clone(),
        };
        let dir = spec.dir.path().to_path_buf();
        task::spawn_blocking(move || intent.write(&dir)).await?.context("write the launch intent")?;
        Ok(Box::new(Handle {
            client: None,
            containers: self.containers.clone(),
            incarnation,
            proxy: None,
            request,
            spec,
            start_timeout: START_TIMEOUT,
            supervisor: Arc::clone(&self.supervisor),
            vmd_bin: self.vmd_bin.clone(),
        }))
    }

    async fn sweep(&self, state: &state::Root) -> Swept {
        let mut swept = reap(self.supervisor.as_ref(), state).await;
        let (cache, images) = (state.cache(), state.images());
        let failures = task::spawn_blocking(move || {
            let blobs = images.join("blobs").join("sha256");
            let dirs = [cache.join("initfs"), cache.join("kernel"), cache.join("rootfs"), blobs];
            dirs.iter().flat_map(|dir| sweep_stale(dir)).collect::<Vec<String>>()
        })
        .await;
        swept.failures.extend(failures.unwrap_or_else(|err| vec![format!("the temporary entry sweep failed: {err}")]));
        swept
    }
}

#[async_trait]
impl WorkloadHandle for Handle {
    async fn release(self: Box<Self>, gone: bool) {
        let Self { containers, spec, supervisor, .. } = *self;
        let confirmed = supervisor.stop(&spec.id, RELEASE_GRACE).await.is_ok();
        if gone && confirmed {
            let vm_state = containers.join(&spec.id);
            drop(task::spawn_blocking(move || remove_tree(&vm_state)).await);
            drop(spec.dir);
        } else {
            spec.dir.leave();
        }
    }

    async fn start_mediator(&mut self, spec: MediatorSpec) -> Result<()> {
        if self.proxy.is_some() {
            bail!("the workload already has a mediator");
        }
        self.proxy = Some(spawn_mediator(&self.spec.dir, &self.spec.id, &spec).await?);
        Ok(())
    }

    async fn start_workload(&mut self) -> Result<()> {
        if self.proxy.is_none() {
            bail!("the workload's mediator is not running");
        }
        let helper_spec = HelperSpec {
            bin: self.vmd_bin.clone(),
            dir: self.spec.dir.path().to_path_buf(),
            log: self.spec.logs_dir.join(format!("{}.avirtvmd.log", self.spec.id)),
        };
        let helper = self.supervisor.start(&self.spec.id, &self.incarnation, &helper_spec).await?;
        let start = helper.request(RequestOp::StartWorkload(Box::new(self.request.clone())));
        let answered = time::timeout(self.start_timeout, start)
            .await
            .map_err(|_| anyhow!("avirtvmd didn't answer the start request within {:?}", self.start_timeout))?;
        match answered? {
            ResponseResult::Started(_) => {}
            ResponseResult::Failure(failure) => bail!("avirtvmd couldn't start the workload: {}", failure.message),
            ResponseResult::Status(_) | ResponseResult::Stopped(_) => {
                bail!("avirtvmd answered a start with something else")
            }
        }
        self.client = Some(helper);
        mark_running(self.spec.dir.path()).await
    }

    async fn stop(&mut self, grace: Duration) -> Stopped {
        let gone = self.supervisor.stop(&self.spec.id, grace).await.is_ok();
        let exit = match &self.client {
            Some(client) => match time::timeout(EXIT_EVENT_WAIT, client.event()).await {
                Ok(Some(event)) => workload_exit(event),
                Ok(None) | Err(_) => Ok(WorkloadExit { code: None, signal: None }),
            },
            None => Ok(WorkloadExit { code: None, signal: None }),
        };
        Stopped { exit, gone }
    }

    async fn stop_mediator(&mut self) {
        if let Some(proxy) = self.proxy.take() {
            proxy.stop().await;
        }
    }

    async fn wait(&mut self) -> Exit {
        let (Some(client), Some(proxy)) = (self.client.as_ref(), self.proxy.as_mut()) else {
            return Exit::Lost("the workload is not running".to_owned());
        };
        tokio::select! {
            event = client.event() => event.map_or_else(
                || Exit::Lost("avirtvmd's control connection closed".to_owned()),
                |exited| Exit::Workload(workload_exit(exited)),
            ),
            died = proxy.exited() => Exit::Mediator(died.map_or_else(|err| err.to_string(), |exit| exit.to_string())),
        }
    }
}

/// How a `WorkloadExited` event says the container ended.
///
/// # Errors
///
/// Returns the helper's error text when it couldn't say how the container ended.
fn workload_exit(event: Event) -> io::Result<WorkloadExit> {
    let Some(EventKind::WorkloadExited(exited)) = event.into_kind() else {
        return Err(io::Error::other("avirtvmd sent an event of an unknown kind"));
    };
    let status = exited.exit.unwrap_or_default();
    if status.code.is_none() && !status.error.is_empty() {
        return Err(io::Error::other(status.error));
    }
    Ok(WorkloadExit { code: status.code, signal: status.signal })
}

/// Records in the launch intent that the helper started the workload.
///
/// # Errors
///
/// Returns an error if reading or writing the intent fails.
async fn mark_running(dir: &Path) -> Result<()> {
    let path = dir.to_path_buf();
    task::spawn_blocking(move || {
        let mut intent = Intent::read(&path)?;
        intent.state = State::Running;
        intent.write(&path)
    })
    .await?
    .context("record that the workload runs")
}
