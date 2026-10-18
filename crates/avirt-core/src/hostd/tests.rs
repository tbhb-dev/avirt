//! Unit tests for the launch environment.

use super::profile::{Cleanup, Config, EnvVar, Prepared, Profile};
use super::{Hostd, LaunchArgs, launch_env};
use crate::runtime::{Exit, MediatorSpec, Runtime, Stopped, Swept, WorkloadExit, WorkloadHandle, WorkloadSpec};
use crate::state;
use alloc::sync::Arc;
use anyhow::{Result, bail, ensure};
use async_trait::async_trait;
use core::time::Duration;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;
use std::sync::{Mutex, PoisonError};
use tokio::sync::Notify;
use tokio::time;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

/// A profile that prepares nothing.
struct Bare;

impl Profile for Bare {
    fn image(&self) -> &'static str {
        "image:dev"
    }

    fn prepare(&self, _scratch: &Path) -> Result<Prepared> {
        Ok(Prepared { cleanup: Cleanup::none(), env: Vec::new(), shares: Vec::new() })
    }
}

/// What the fake runtime saw.
#[derive(Default)]
struct Seen {
    /// Signalled when a handle releases.
    released: Notify,
    /// The `gone` argument of each release.
    releases: Mutex<Vec<bool>>,
}

/// A runtime that holds `open` until the test lets it go, and whose handle fails to start its mediator.
struct Gated {
    /// Lets `open` return.
    gate: Notify,
    /// Signalled when `open` is waiting at the gate.
    opening: Notify,
    /// What happened.
    seen: Arc<Seen>,
}

/// The handle that `Gated` opens.
struct Failing(Arc<Seen>);

#[async_trait]
impl Runtime for Gated {
    async fn open(&self, _spec: WorkloadSpec) -> Result<Box<dyn WorkloadHandle>> {
        self.opening.notify_one();
        self.gate.notified().await;
        Ok(Box::new(Failing(Arc::clone(&self.seen))))
    }

    async fn sweep(&self, _state: &state::Root) -> Swept {
        Swept::default()
    }
}

#[async_trait]
impl WorkloadHandle for Failing {
    async fn release(self: Box<Self>, gone: bool) {
        self.0.releases.lock().unwrap_or_else(PoisonError::into_inner).push(gone);
        self.0.released.notify_one();
    }

    async fn start_mediator(&mut self, _spec: MediatorSpec) -> Result<()> {
        bail!("the mediator can't start")
    }

    async fn start_workload(&mut self) -> Result<()> {
        bail!("never reached")
    }

    async fn stop(&mut self, _grace: Duration) -> Stopped {
        Stopped { exit: Ok(WorkloadExit { code: None, signal: None }), gone: true }
    }

    async fn stop_mediator(&mut self) {}

    async fn wait(&mut self) -> Exit {
        Exit::Lost("never started".to_owned())
    }
}

/// The value of `name` in `vars`, if it's set.
fn value<'vars>(vars: &'vars [(String, String)], name: &str) -> Option<&'vars str> {
    vars.iter().find(|(var, _)| var == name).map(|(_, text)| text.as_str())
}

/// Inherited and fixed variables resolve, and avirthostd's own follow them.
///
/// # Errors
///
/// Fails when a variable is missing or has the wrong value.
#[test]
fn launch_env_resolves_profile_variables_and_adds_the_avirt_ones() -> Result<()> {
    let profile = [
        EnvVar::Inherit("PATH"),
        EnvVar::Inherit("AVIRT_SURELY_UNSET_VARIABLE"),
        EnvVar::Set("CODEX_HOME", "/codex-home".to_owned()),
    ];
    let args = LaunchArgs { agent: "claude", cwd: Some(Path::new("/work")), prompt: "hello" };
    let vars = launch_env(&profile, &args, "0000000a", "tok")?;
    ensure!(value(&vars, "PATH") == env::var("PATH").ok().as_deref(), "PATH wasn't inherited");
    ensure!(value(&vars, "AVIRT_SURELY_UNSET_VARIABLE").is_none(), "an unset variable resolved");
    ensure!(value(&vars, "CODEX_HOME") == Some("/codex-home"), "the fixed variable is missing");
    ensure!(value(&vars, "AVIRT_WORKLOAD_ID") == Some("0000000a"), "the workload id is missing");
    ensure!(value(&vars, "AVIRT_CONNECTION_TOKEN") == Some("tok"), "the token is missing");
    ensure!(value(&vars, "AVIRT_PROMPT") == Some("hello"), "the prompt is missing");
    ensure!(value(&vars, "AVIRT_CWD") == Some("/workspace"), "the working directory is missing");
    let bare = LaunchArgs { agent: "claude", cwd: None, prompt: "hello" };
    ensure!(value(&launch_env(&[], &bare, "0000000a", "tok")?, "AVIRT_CWD") == Some(""), "AVIRT_CWD should be empty");
    Ok(())
}

/// A launch whose caller goes away mid-start still tears the workload down.
///
/// # Errors
///
/// Fails when the runtime never releases the handle after the caller drops its future.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_dropped_launch_still_runs_to_its_end() -> Result<()> {
    let base = tempfile::tempdir()?;
    let state = state::Root::open_in(base.path())?;
    let seen = Arc::new(Seen::default());
    let runtime = Arc::new(Gated { gate: Notify::new(), opening: Notify::new(), seen: Arc::clone(&seen) });
    let profile: Box<dyn Profile> = Box::new(Bare);
    let profiles = HashMap::from([("bare".to_owned(), profile)]);
    let shared: Arc<dyn Runtime> = Arc::<Gated>::clone(&runtime);
    let daemon = Arc::new(Hostd {
        config: Config { logs_dir: base.path().join("logs"), mediator_bin: base.path().join("mediator"), port: 0 },
        profiles,
        runtime: shared,
        shutdown: CancellationToken::new(),
        socket: base.path().join("hostd.sock"),
        state,
        tombstones: Mutex::default(),
        tracker: TaskTracker::new(),
        workloads: Mutex::default(),
    });
    fs::create_dir_all(base.path().join("logs"))?;

    let caller = tokio::spawn({
        let worker = Arc::clone(&daemon);
        async move { worker.launch_detached("bare".to_owned(), "hi".to_owned(), None).await }
    });
    runtime.opening.notified().await;
    caller.abort();
    ensure!(caller.await.is_err(), "the caller should have been cancelled");
    runtime.gate.notify_one();

    time::timeout(Duration::from_secs(5), seen.released.notified()).await?;
    let releases = seen.releases.lock().unwrap_or_else(PoisonError::into_inner).clone();
    ensure!(releases == [true], "the handle was released {releases:?}");
    ensure!(
        daemon.workloads.lock().unwrap_or_else(PoisonError::into_inner).is_empty(),
        "the workload stayed registered"
    );
    Ok(())
}
