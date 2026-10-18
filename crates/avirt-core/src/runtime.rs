//! How avirthostd runs a workload. A [`Runtime`] opens one [`WorkloadHandle`] per workload. The
//! handle starts the workload's mediator and container, then reports how they end and removes
//! what they left.

pub mod container_cli;
pub mod containerization;

use crate::egress::{Proxy, ProxySpec, WorkloadDir};
use crate::hostd::profile::Share;
use crate::state;
use anyhow::Result;
use async_trait::async_trait;
use core::time::Duration;
use std::io;
use std::path::PathBuf;

/// A variable's name and value.
pub type EnvPair = (String, String);

/// Everything a runtime needs to run one workload container. Owned, so a handle can keep it.
#[derive(Debug)]
pub struct WorkloadSpec {
    /// The workload's 0700 directory. The handle owns it from `open` on and removes it in `release`.
    pub dir: WorkloadDir,
    /// Profile variables plus avirthostd's own (`AVIRT_WORKLOAD_ID`, `AVIRT_PROMPT`, `AVIRT_CWD`,
    /// `AVIRT_CONNECTION_TOKEN`), with the values avirthostd resolved. Never logged, and a
    /// runtime never puts a value on a command line.
    pub env: Vec<EnvPair>,
    /// The workload's id.
    pub id: String,
    /// The image reference the profile names, such as `avirt-workload-claude:dev`.
    pub image: String,
    /// The absolute logs directory.
    pub logs_dir: PathBuf,
    /// Host directories shared into the container, read-write.
    pub shares: Vec<Share>,
}

/// What a runtime needs to start the workload's mediator.
#[derive(Debug)]
pub struct MediatorSpec {
    /// The `avirtmediatord` binary.
    pub bin: PathBuf,
    /// avirthostd's own socket, which the mediator dials.
    pub hostd_sock: PathBuf,
    /// Where the mediator's stderr goes. The file must not exist yet.
    pub log: PathBuf,
    /// The secret the mediator presents in its connection preambles.
    pub secret: String,
}

/// How a workload container ended.
#[derive(Clone, Copy, Debug)]
pub struct WorkloadExit {
    /// The exit code, if the container exited normally.
    pub code: Option<i32>,
    /// The signal that ended the container, if one did.
    pub signal: Option<i32>,
}

/// Why [`WorkloadHandle::wait`] returned.
#[derive(Debug)]
pub enum Exit {
    /// The runtime lost the workload. Holds the reason.
    Lost(String),
    /// The mediator exited on its own while the workload ran. Holds the exit status text.
    Mediator(String),
    /// The workload container exited, or the runtime can't say how.
    Workload(io::Result<WorkloadExit>),
}

/// The outcome of [`WorkloadHandle::stop`].
#[derive(Debug)]
pub struct Stopped {
    /// How the container ended.
    pub exit: io::Result<WorkloadExit>,
    /// True only when the runtime confirms that no part of the workload container still runs.
    pub gone: bool,
}

/// What a startup sweep did.
#[derive(Debug, Default)]
pub struct Swept {
    /// Failures to remove something. The sweep goes on without it.
    pub failures: Vec<String>,
    /// Workload ids whose directories must stay, because the runtime can't confirm them gone.
    pub kept: Vec<String>,
    /// What the sweep stopped and deleted.
    pub removed: Vec<String>,
}

/// A way to run workloads.
#[async_trait]
pub trait Runtime: Send + Sync {
    /// Allocates the workload's runtime resources without starting the agent.
    ///
    /// # Errors
    ///
    /// Returns an error if the runtime can't set up the workload.
    async fn open(&self, spec: WorkloadSpec) -> Result<Box<dyn WorkloadHandle>>;

    /// Runs under the state root's lock before avirthostd listens. Ends everything a previous
    /// avirthostd left running.
    async fn sweep(&self, state: &state::Root) -> Swept;
}

/// One workload under a [`Runtime`].
#[async_trait]
pub trait WorkloadHandle: Send {
    /// Removes the workload's runtime state and its directory. With `gone` false it leaves the
    /// directory on disk for the next startup's sweep.
    async fn release(self: Box<Self>, gone: bool);

    /// Starts the mediator and returns once its sockets accept connections. Fails while a
    /// mediator already runs.
    ///
    /// # Errors
    ///
    /// Returns an error if the mediator can't start or never accepts connections.
    async fn start_mediator(&mut self, spec: MediatorSpec) -> Result<()>;

    /// Starts the workload container. Requires a running mediator.
    ///
    /// # Errors
    ///
    /// Returns an error if the mediator isn't running or the runtime can't prepare the launch.
    async fn start_workload(&mut self) -> Result<()>;

    /// Asks the container to stop, then forces it after `grace`. Idempotent.
    async fn stop(&mut self, grace: Duration) -> Stopped;

    /// Closes the mediator's stdin and waits for it to flush its events (5 s cap, then kills it).
    async fn stop_mediator(&mut self);

    /// Resolves when the workload container exits, the mediator exits, or the runtime loses the
    /// workload. Cancel-safe.
    async fn wait(&mut self) -> Exit;
}

/// Starts a workload's mediator, the same way for every runtime.
///
/// # Errors
///
/// Returns an error if the mediator can't start or never accepts connections.
pub(crate) async fn spawn_mediator(dir: &WorkloadDir, id: &str, spec: &MediatorSpec) -> Result<Proxy> {
    Proxy::spawn(&ProxySpec {
        bin: &spec.bin,
        dir,
        hostd_sock: &spec.hostd_sock,
        id,
        log_path: &spec.log,
        secret: &spec.secret,
    })
    .await
}
