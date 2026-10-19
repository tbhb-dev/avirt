//! The agent profiles and the per-launch setup they return.

use anyhow::Result;
use core::error::Error;
use core::fmt;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// A one-shot cleanup closure that returns the meta message for the workload log, if any.
type CleanupFn = Box<dyn FnOnce() -> Option<Value> + Send>;

/// A one-shot cleanup that runs exactly once: explicitly through [`Cleanup::run`] or, failing
/// that, when dropped (setup failure partway, avirthostd unwinding).
pub struct Cleanup(Option<CleanupFn>);

/// What a [`Profile`] sets up for one launch.
#[derive(Debug)]
pub struct Prepared {
    /// The cleanup to run once the container no longer exists.
    pub cleanup: Cleanup,
    /// Variables for the workload container. A runtime applies them without putting a value on argv.
    pub env: Vec<EnvVar>,
    /// Host directories shared into the workload container, read-write.
    pub shares: Vec<Share>,
}

/// A variable for the workload container.
#[derive(Clone, Debug)]
pub enum EnvVar {
    /// Takes avirthostd's own value at launch. Only these avirthostd environment variables pass
    /// through, and secrets always use this form.
    Inherit(&'static str),
    /// A fixed, non-secret value.
    Set(&'static str, String),
}

/// A host directory shared into the workload container.
#[derive(Clone, Debug)]
pub struct Share {
    /// Where it appears in the container.
    pub guest: &'static str,
    /// The directory on the host.
    pub host: PathBuf,
}

/// A launch avirthostd refuses for a reason the caller can fix (missing credentials, say).
/// avirthostd reports a 400 with this message. Any other `prepare` error is a 500.
#[derive(Debug)]
pub struct Refused(
    /// The message reported to the caller.
    pub String,
);

/// What differs between agents: the image and the per-launch container setup.
pub trait Profile: Send + Sync {
    /// The container image the agent runs in.
    fn image(&self) -> &str;

    /// Per-launch setup: environment variables and shared directories for the workload container,
    /// plus the cleanup to run once the container no longer exists. Dropping the returned [`Prepared`] without launching runs the cleanup.
    ///
    /// `scratch` is a directory under avirthostd's state root for per-launch temp files, so that
    /// an avirthostd process killed uncleanly leaves them where the next one sweeps them.
    ///
    /// # Errors
    ///
    /// Returns [`Refused`] for a launch the caller can fix, such as missing credentials. Any
    /// other error reports a 500.
    fn prepare(&self, scratch: &Path) -> Result<Prepared>;
}

/// Configuration for [`serve`](super::serve).
#[derive(Debug)]
pub struct Config {
    /// Where avirthostd writes each workload's NDJSON log.
    pub logs_dir: PathBuf,
    /// The `avirtmediatord` binary started for each workload.
    pub mediator_bin: PathBuf,
    /// The loopback port of the control API.
    pub port: u16,
}

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for Refused {}

impl fmt::Debug for Cleanup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Cleanup").field("armed", &self.0.is_some()).finish()
    }
}

impl Cleanup {
    /// Gives up on the cleanup without running it, and without dropping what it owns, so a
    /// temp directory it holds stays on disk for the next startup's sweep. Used when the
    /// caller couldn't confirm that the container no longer exists. A write-back then could race a live agent.
    pub fn abandon(&mut self) {
        if let Some(cleanup) = self.0.take() {
            Box::leak(cleanup);
        }
    }

    /// A cleanup that runs `cleanup` once, when run or dropped.
    pub fn new<F>(cleanup: F) -> Self
    where
        F: FnOnce() -> Option<Value> + Send + 'static,
    {
        Self(Some(Box::new(cleanup)))
    }

    /// A cleanup that does nothing.
    #[must_use]
    pub const fn none() -> Self {
        Self(None)
    }

    /// Runs the cleanup. The returned meta message, if any, belongs in the workload log.
    pub fn run(&mut self) -> Option<Value> {
        let cleanup = self.0.take()?;
        cleanup()
    }
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        drop(self.run());
    }
}
