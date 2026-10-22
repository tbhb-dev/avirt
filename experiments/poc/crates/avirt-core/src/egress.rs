//! Host side of the mediator: a private directory and one `avirtmediatord` process per workload.
//!
//! The mediator listens on two sockets in that directory, which are bind-mounted into the
//! container (the runtime relays a mounted socket file to its host listener), and dials
//! avirthostd's own socket.

#[cfg(test)]
mod tests;

use core::time::Duration;
use std::env;
use std::fs::{self, DirBuilder, OpenOptions};
use std::io;
use std::os::unix::fs::{DirBuilderExt as _, OpenOptionsExt as _};
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};

use anyhow::{Context as _, Result, bail};
use tokio::net::UnixStream;
use tokio::process::{Child, ChildStdin, Command};
use tokio::time::{self, Instant};

/// Longest path `sockaddr_un` can hold on macOS, including the final NUL.
const SUN_PATH_MAX: usize = 104;

/// The sockets in a workload directory. Every path must fit, and the longest name is the one at risk.
const SOCKET_NAMES: [&str; 3] = ["proxy.sock", "mediator.sock", "v.sock"];

/// A workload's private directory (0700), `workloads/<workload-id>/`.
///
/// It holds the proxy socket and the mediator socket. Dropping it removes the directory and
/// whatever is still in it.
#[derive(Debug)]
pub struct WorkloadDir(PathBuf);

impl WorkloadDir {
    /// Creates `parent/<id>` (the state root's `workloads/`).
    ///
    /// # Errors
    ///
    /// Returns an error if the directory already exists or creating it fails, or if a socket path
    /// inside it would not fit in `sockaddr_un`.
    pub fn create(parent: &Path, id: &str) -> Result<Self> {
        let path = parent.join(id);
        for name in SOCKET_NAMES {
            let socket = path.join(name);
            if socket.as_os_str().len() >= SUN_PATH_MAX {
                bail!("{} is too long for a Unix socket path", socket.display());
            }
        }
        DirBuilder::new().mode(0o700).create(&path)?;
        Ok(Self(path))
    }

    /// Gives up the directory without removing it, so the next startup's sweep does.
    pub fn leave(self) {
        Box::leak(Box::new(self));
    }

    /// The container's workload connection socket, mounted at `/run/avirt/mediator.sock`.
    #[must_use]
    pub fn mediator_socket(&self) -> PathBuf {
        self.0.join("mediator.sock")
    }

    /// The directory's path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.0
    }

    /// The container's CONNECT proxy socket, mounted at `/run/avirt/proxy.sock`.
    #[must_use]
    pub fn proxy_socket(&self) -> PathBuf {
        self.0.join("proxy.sock")
    }
}

impl Drop for WorkloadDir {
    fn drop(&mut self) {
        drop(fs::remove_dir_all(&self.0));
    }
}

/// A running `avirtmediatord`.
///
/// Never write to its stdin. Closing it (or avirthostd dying) shuts it down.
#[derive(Debug)]
pub struct Proxy {
    /// The mediator process.
    child: Child,
    /// The mediator's stdin, held open until `stop`.
    stdin: Option<ChildStdin>,
}

/// What [`Proxy::spawn`] needs to start one workload's mediator.
#[derive(Clone, Copy, Debug)]
pub struct ProxySpec<'spec> {
    /// The `avirtmediatord` binary.
    pub bin: &'spec Path,
    /// The workload's private directory, which holds the mediator's sockets.
    pub dir: &'spec WorkloadDir,
    /// avirthostd's own socket, which the mediator dials.
    pub hostd_sock: &'spec Path,
    /// The workload's identifier.
    pub id: &'spec str,
    /// Where the mediator's stderr goes. The file must not exist yet.
    pub log_path: &'spec Path,
    /// The secret the mediator presents in its connection preambles.
    pub secret: &'spec str,
}

impl Proxy {
    /// Resolves when the mediator exits on its own, which is a failure while a workload runs.
    ///
    /// # Errors
    ///
    /// Returns an error if waiting on the process fails.
    pub async fn exited(&mut self) -> io::Result<ExitStatus> {
        self.child.wait().await
    }

    /// Starts the mediator for the spec's directory and waits up to 3 s for both sockets to
    /// accept connections.
    ///
    /// The sockets must exist before avirthostd starts the container, because the runtime only
    /// mounts a socket file that's already there.
    ///
    /// # Errors
    ///
    /// Returns an error if the log file already exists, the mediator won't start, or it exits or
    /// times out before both sockets accept connections.
    pub async fn spawn(spec: &ProxySpec<'_>) -> Result<Self> {
        let ProxySpec { bin, dir, hostd_sock, id, log_path, secret } = *spec;
        let stderr = OpenOptions::new().create_new(true).append(true).mode(0o600).open(log_path)?;
        let mut child = Command::new(bin)
            // Only what it needs: no credentials from avirthostd's environment.
            .env_clear()
            .envs(["PATH", "HOME", "TMPDIR"].iter().filter_map(|name| env::var_os(name).map(|value| (*name, value))))
            .env("AVIRT_MEDIATOR_SECRET", secret)
            .arg("--dir")
            .arg(dir.path())
            .arg("--hostd-sock")
            .arg(hostd_sock)
            .args(["--workload", id])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(stderr)
            .kill_on_drop(true)
            // A separate process group prevents terminal Ctrl-C from reaching the mediator while avirthostd receives it.
            .process_group(0)
            .spawn()
            .with_context(|| format!("spawn {}", bin.display()))?;
        let stdin = child.stdin.take();
        let mut proxy = Self { child, stdin };

        let started = Instant::now();
        loop {
            // A probe connects and closes without a byte. The mediator ignores it.
            let ready = UnixStream::connect(dir.proxy_socket()).await.is_ok()
                && UnixStream::connect(dir.mediator_socket()).await.is_ok();
            if ready {
                return Ok(proxy);
            }
            if proxy.child.try_wait()?.is_some() || started.elapsed() >= Duration::from_secs(3) {
                proxy.stop().await;
                bail!("the mediator did not start (see {})", log_path.display());
            }
            time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// Closes stdin and waits for the mediator to flush its events and exit (5 s cap, then
    /// kills it).
    pub async fn stop(mut self) {
        drop(self.stdin.take());
        if time::timeout(Duration::from_secs(5), self.child.wait()).await.is_err() {
            drop(self.child.kill().await);
            drop(self.child.wait().await);
        }
    }
}
