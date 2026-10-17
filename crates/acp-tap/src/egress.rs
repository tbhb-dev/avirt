//! Host side of the supervisor: a private directory and one `acp-supervisor` process per session.
//! The supervisor listens on two sockets in that directory, which are bind-mounted into the
//! container (the runtime relays a mounted socket file to its host listener), and dials the
//! daemon's own socket.

use std::fs::{OpenOptions, Permissions};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use tempfile::TempDir;
use tokio::process::{Child, ChildStdin, Command};

/// A session's private directory (0700) holding the proxy socket and the uplink socket. It is
/// removed, with whatever is still in it, when dropped.
#[derive(Debug)]
pub struct SessionDir(TempDir);

impl SessionDir {
    /// Created under `parent` (the state root's `sessions/`), which keeps the socket paths short
    /// enough for `sockaddr_un`.
    pub fn create(parent: &Path) -> Result<Self> {
        let dir =
            tempfile::Builder::new().prefix("s-").permissions(Permissions::from_mode(0o700)).tempdir_in(parent)?;
        Ok(Self(dir))
    }

    /// The container's CONNECT proxy socket, mounted at `/run/acp/proxy.sock`.
    #[must_use]
    pub fn proxy_socket(&self) -> PathBuf {
        self.0.path().join("proxy.sock")
    }

    /// The container's session uplink socket, mounted at `/run/acp/session.sock`.
    #[must_use]
    pub fn session_socket(&self) -> PathBuf {
        self.0.path().join("session.sock")
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        self.0.path()
    }
}

/// A running `acp-supervisor`. Its stdin is never written; closing it (or the daemon dying)
/// shuts it down.
#[derive(Debug)]
pub struct Proxy {
    child: Child,
    stdin: Option<ChildStdin>,
}

impl Proxy {
    /// Starts the supervisor for `dir` and waits up to 3 s for both sockets to accept
    /// connections. The sockets must exist before the container is started, because the runtime
    /// only mounts a socket file that is already there.
    pub async fn spawn(
        bin: &Path,
        id: &str,
        dir: &SessionDir,
        daemon_sock: &Path,
        secret: &str,
        log_path: &Path,
    ) -> Result<Self> {
        let stderr = OpenOptions::new().create_new(true).append(true).mode(0o600).open(log_path)?;
        let mut child = Command::new(bin)
            // Only what it needs: no credentials from the daemon's environment.
            .env_clear()
            .envs(["PATH", "HOME", "TMPDIR"].iter().filter_map(|n| std::env::var_os(n).map(|v| (*n, v))))
            .env("ACP_SUPERVISOR_SECRET", secret)
            .arg("--dir")
            .arg(dir.path())
            .arg("--daemon-sock")
            .arg(daemon_sock)
            .args(["--session", id])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(stderr)
            .kill_on_drop(true)
            // Own process group, so a terminal Ctrl-C reaches the daemon and not the supervisor.
            .process_group(0)
            .spawn()
            .with_context(|| format!("spawn {}", bin.display()))?;
        let stdin = child.stdin.take();
        let mut proxy = Self { child, stdin };

        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        loop {
            // A probe connects and closes without a byte; the supervisor ignores it.
            let ready = tokio::net::UnixStream::connect(dir.proxy_socket()).await.is_ok()
                && tokio::net::UnixStream::connect(dir.session_socket()).await.is_ok();
            if ready {
                return Ok(proxy);
            }
            if proxy.child.try_wait()?.is_some() || tokio::time::Instant::now() >= deadline {
                proxy.stop().await;
                bail!("the supervisor did not start (see {})", log_path.display());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// Resolves when the supervisor exits on its own, which is a failure while a session runs.
    pub async fn exited(&mut self) -> std::io::Result<ExitStatus> {
        self.child.wait().await
    }

    /// Closes stdin and waits for the supervisor to flush its events and exit (5 s cap, then
    /// kills it).
    pub async fn stop(mut self) {
        drop(self.stdin.take());
        if tokio::time::timeout(Duration::from_secs(5), self.child.wait()).await.is_err() {
            let _ = self.child.kill().await;
            let _ = self.child.wait().await;
        }
    }
}
