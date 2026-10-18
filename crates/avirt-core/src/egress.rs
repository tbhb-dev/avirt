//! Host side of the mediator: a private directory and one `avirtmediatord` process per workload.
//! The mediator listens on two sockets in that directory, which are bind-mounted into the
//! container (the runtime relays a mounted socket file to its host listener), and dials
//! avirthostd's own socket.

use std::fs::OpenOptions;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use tokio::process::{Child, ChildStdin, Command};

/// A workload's private directory (0700), `workloads/<workload-id>/`. It holds the proxy socket and the
/// mediator socket. Dropping it removes the directory and whatever is still in it.
#[derive(Debug)]
pub struct WorkloadDir(PathBuf);

/// Longest path `sockaddr_un` can hold on macOS, including the final NUL.
const SUN_PATH_MAX: usize = 104;

impl WorkloadDir {
    /// Created as `parent/<id>` (the state root's `workloads/`). Fails if it already exists or if a
    /// socket path inside it would not fit in `sockaddr_un`.
    pub fn create(parent: &Path, id: &str) -> Result<Self> {
        let path = parent.join(id);
        let longest = path.join("proxy.sock");
        if longest.as_os_str().len() >= SUN_PATH_MAX {
            bail!("{} is too long for a Unix socket path", longest.display());
        }
        std::fs::DirBuilder::new().mode(0o700).create(&path)?;
        Ok(Self(path))
    }

    /// The container's CONNECT proxy socket, mounted at `/run/avirt/proxy.sock`.
    #[must_use]
    pub fn proxy_socket(&self) -> PathBuf {
        self.0.join("proxy.sock")
    }

    /// The container's workload connection socket, mounted at `/run/avirt/mediator.sock`.
    #[must_use]
    pub fn mediator_socket(&self) -> PathBuf {
        self.0.join("mediator.sock")
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for WorkloadDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A running `avirtmediatord`. Never write to its stdin. Closing it (or avirthostd dying)
/// shuts it down.
#[derive(Debug)]
pub struct Proxy {
    child: Child,
    stdin: Option<ChildStdin>,
}

impl Proxy {
    /// Starts the mediator for `dir` and waits up to 3 s for both sockets to accept
    /// connections. The sockets must exist before avirthostd starts the container, because the runtime
    /// only mounts a socket file that's already there.
    pub async fn spawn(
        bin: &Path,
        id: &str,
        dir: &WorkloadDir,
        hostd_sock: &Path,
        secret: &str,
        log_path: &Path,
    ) -> Result<Self> {
        let stderr = OpenOptions::new().create_new(true).append(true).mode(0o600).open(log_path)?;
        let mut child = Command::new(bin)
            // Only what it needs: no credentials from avirthostd's environment.
            .env_clear()
            .envs(
                ["PATH", "HOME", "TMPDIR", "AVIRT_PROXY_ENGINE", "AVIRT_ENVOY_BIN", "AVIRT_ENVOY_MODULE"]
                    .iter()
                    .filter_map(|n| std::env::var_os(n).map(|v| (*n, v))),
            )
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

        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        loop {
            // A probe connects and closes without a byte. The mediator ignores it.
            let ready = tokio::net::UnixStream::connect(dir.proxy_socket()).await.is_ok()
                && tokio::net::UnixStream::connect(dir.mediator_socket()).await.is_ok();
            if ready {
                return Ok(proxy);
            }
            if proxy.child.try_wait()?.is_some() || tokio::time::Instant::now() >= deadline {
                proxy.stop().await;
                bail!("the mediator did not start (see {})", log_path.display());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// Resolves when the mediator exits on its own, which is a failure while a workload runs.
    pub async fn exited(&mut self) -> std::io::Result<ExitStatus> {
        self.child.wait().await
    }

    /// Closes stdin and waits for the mediator to flush its events and exit (5 s cap, then
    /// kills it).
    pub async fn stop(mut self) {
        drop(self.stdin.take());
        if tokio::time::timeout(Duration::from_secs(5), self.child.wait()).await.is_err() {
            let _ = self.child.kill().await;
            let _ = self.child.wait().await;
        }
    }
}
