//! Starting, finding, and stopping `avirtvmd` helpers.
//!
//! A helper is a child process of avirthostd that lives on avirthostd's stdin: it exits when that
//! pipe closes.

#[cfg(test)]
mod real_tests;
#[cfg(test)]
mod tests;

use super::control::{self, Client, Expected, HELLO_TIMEOUT, HandshakeError};
use super::intent::{Intent, State};
use super::proto::{Event, RequestOp, ResponseResult, Stop};
use anyhow::{Context as _, Result, anyhow, bail};
use async_trait::async_trait;
use core::fmt;
use core::time::Duration;
use rustix::process::{self, Pid, Signal};
use std::collections::HashMap;
use std::env;
use std::fs::{File, OpenOptions, TryLockError};
use std::io;
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Mutex, MutexGuard, PoisonError};
use tokio::fs;
use tokio::io::AsyncWriteExt as _;
use tokio::process::{Child, ChildStdin, Command};
use tokio::task;
use tokio::time::{self, Instant};

/// How long the helper has to bind its socket after it starts.
const BIND_TIMEOUT: Duration = Duration::from_secs(10);
/// How long to wait for a helper that another avirthostd started to answer.
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);
/// How long to wait for a helper to exit after SIGKILL.
const KILL_WAIT: Duration = Duration::from_secs(5);
/// Slack beyond the grace period for the helper to stop its VM and exit.
const EXIT_SLACK: Duration = Duration::from_secs(5);

/// 16 random bytes that avirthostd gives each helper start.
///
/// The helper presents them in its hello, so a leftover helper of the same workload can't pass
/// for the one avirthostd started.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct Incarnation([u8; 16]);

/// What starting a helper needs.
#[derive(Clone, Debug)]
pub struct HelperSpec {
    /// The signed avirtvmd binary.
    pub bin: PathBuf,
    /// `workloads/<id>/`, where avirtvmd binds `v.sock` and holds `vmd.lock`.
    pub dir: PathBuf,
    /// `logs/<id>.avirtvmd.log`, avirtvmd's stderr.
    pub log: PathBuf,
}

/// What avirthostd knows about one helper.
#[derive(Debug, Eq, PartialEq)]
pub enum HelperState {
    /// The helper exited, and its lock is free.
    Ended,
    /// Alive and answering with a protocol version avirthostd doesn't support.
    Incompatible(String),
    /// The helper answers and matches its launch intent.
    Running,
    /// Alive, because it holds its lock, but not answering. The cause can be a missing socket, a
    /// timeout, or a failed listing.
    Unknown(String),
}

/// One helper in a [`HelperSupervisor::list`].
#[derive(Debug)]
pub struct HelperStatus {
    /// The incarnation in the launch intent, if the intent was readable.
    pub incarnation: Option<Incarnation>,
    /// The state in the launch intent, if the intent was readable.
    pub recorded: Option<State>,
    /// What avirthostd knows about the helper.
    pub state: HelperState,
    /// The workload's id.
    pub workload: String,
}

/// The control connection to one helper, after the handshake.
#[derive(Clone, Debug)]
pub struct HelperHandle(Client);

/// Starts, finds, and stops helpers.
#[async_trait]
pub trait HelperSupervisor: Send + Sync {
    /// Every helper this supervisor knows about, with its last known state, for reconciliation.
    ///
    /// # Errors
    ///
    /// Returns an error if reading the listing fails. It never answers an empty list for that.
    async fn list(&self) -> Result<Vec<HelperStatus>>;

    /// Starts the helper for one workload. Idempotent for the same workload and incarnation.
    ///
    /// # Errors
    ///
    /// Returns an error if the workload has a helper with another incarnation, or the helper
    /// fails to start, bind its socket, or complete the handshake.
    async fn start(&self, workload: &str, incarnation: &Incarnation, spec: &HelperSpec) -> Result<HelperHandle>;

    /// Stops and deregisters the helper. Idempotent. Returns `Ok` after confirming that the helper
    /// has exited.
    ///
    /// # Errors
    ///
    /// Returns an error if the helper still runs after the grace period and SIGKILL, or if nothing
    /// shows whether it runs.
    async fn stop(&self, workload: &str, grace: Duration) -> Result<()>;
}

/// What `<dir>/vmd.lock` says about the helper for the directory.
#[derive(Debug, Eq, PartialEq)]
enum LockState {
    /// Checking the lock failed.
    Failed(String),
    /// The lock is free. The helper and its VM have exited.
    Free,
    /// A live helper holds the lock.
    Held,
    /// The lock file is absent, so no helper ever started.
    Missing,
}

/// One started helper.
#[derive(Debug)]
struct Record {
    /// Held for its drop: closing it tells the helper that avirthostd has exited.
    _stdin: ChildStdin,
    /// The helper process.
    child: Child,
    /// The control connection.
    handle: HelperHandle,
    /// The incarnation that the helper started with.
    incarnation: Incarnation,
}

/// Runs each helper as a child process of avirthostd.
#[derive(Debug)]
pub struct ChildSupervisor {
    /// How long to wait for a helper that this supervisor didn't start to answer.
    probe_timeout: Duration,
    /// The helpers this supervisor started, by workload id.
    records: Mutex<HashMap<String, Record>>,
    /// The state root's `workloads/` directory.
    workloads_dir: PathBuf,
}

impl Incarnation {
    /// The 16 bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// Parses exactly 32 lowercase hexadecimal characters.
    #[must_use]
    pub fn from_hex(text: &str) -> Option<Self> {
        let lowercase_hex = text.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
        if text.len() != 32 || !lowercase_hex {
            return None;
        }
        u128::from_str_radix(text, 16).ok().map(|number| Self(number.to_be_bytes()))
    }

    /// A fresh random incarnation.
    #[must_use]
    pub fn random() -> Self {
        Self(rand::random::<u128>().to_be_bytes())
    }

    /// 32 lowercase hexadecimal characters.
    #[must_use]
    pub fn to_hex(self) -> String {
        format!("{:032x}", u128::from_be_bytes(self.0))
    }
}

/// An incarnation identifies one helper start but isn't a secret, so it stays out of logs anyway.
impl fmt::Debug for Incarnation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Incarnation(..)")
    }
}

impl HelperHandle {
    /// The next `Event`, or `None` after the connection closes.
    pub async fn event(&self) -> Option<Event> {
        self.0.event().await
    }

    /// Whether the connection has closed.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.0.is_closed()
    }

    /// Sends one request and waits for the response.
    ///
    /// # Errors
    ///
    /// Returns an error if the connection has closed or closes before the helper answers.
    pub async fn request(&self, op: RequestOp) -> Result<ResponseResult> {
        self.0.request(op).await
    }
}

impl ChildSupervisor {
    /// Dials `v.sock` until the helper has bound it, failing at once if the helper exits first.
    ///
    /// # Errors
    ///
    /// Returns an error if the helper exits first, never binds the socket, or fails the handshake.
    async fn dial(child: &mut Child, dir: &Path, expected: &Expected<'_>) -> Result<Client> {
        let socket = dir.join("v.sock");
        let started = Instant::now();
        loop {
            if let Some(status) = child.try_wait()? {
                bail!("avirtvmd exited during startup ({status})");
            }
            match control::connect(&socket, expected, HELLO_TIMEOUT).await {
                Ok(client) => return Ok(client),
                Err(HandshakeError::Connect(_)) if started.elapsed() < BIND_TIMEOUT => {
                    time::sleep(Duration::from_millis(50)).await;
                }
                Err(err) => return Err(err.into()),
            }
        }
    }

    /// Sends the helper its incarnation on stdin, then dials it.
    ///
    /// A failed write means the helper has closed its stdin, and that counts as the helper exiting
    /// during startup. The intent records the spawn before this function reports either failure.
    ///
    /// # Errors
    ///
    /// Returns an error if the helper exits first, never binds the socket, fails the handshake, or
    /// the intent can't record the spawn.
    async fn handshake(
        child: &mut Child,
        stdin: &mut ChildStdin,
        expected: &Expected<'_>,
        spec: &HelperSpec,
    ) -> Result<Client> {
        // The first line is the incarnation. After it avirthostd writes nothing more, and the
        // helper treats the end of this pipe as avirthostd's death.
        let sent = async {
            stdin.write_all(format!("{}\n", expected.incarnation.to_hex()).as_bytes()).await?;
            stdin.flush().await
        }
        .await;
        Self::record_spawn(&spec.dir, child.id()).await?;
        if let Err(err) = sent {
            // A closed pipe can precede the exit by a moment. A helper that closed it and kept running
            // never read the incarnation, so the write error is the better report.
            return match time::timeout(BIND_TIMEOUT, child.wait()).await {
                Ok(status) => bail!("avirtvmd exited during startup ({})", status?),
                Err(_) => Err(err).context("write the incarnation to avirtvmd"),
            };
        }
        Self::dial(child, &spec.dir, expected).await
    }

    /// Opens the helper's log and starts the helper.
    ///
    /// # Errors
    ///
    /// Returns an error if the log exists already or the helper fails to start.
    fn launch(workload: &str, spec: &HelperSpec) -> Result<(Child, ChildStdin)> {
        let log = OpenOptions::new().create_new(true).append(true).mode(0o600).open(&spec.log)?;
        let mut child = Command::new(&spec.bin)
            .args(["run", "--workload", workload, "--dir"])
            .arg(&spec.dir)
            // Only what it needs: no credentials from avirthostd's environment.
            .env_clear()
            .envs(["PATH", "HOME", "TMPDIR"].iter().filter_map(|name| env::var_os(name).map(|value| (*name, value))))
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(log)
            .kill_on_drop(true)
            // A separate process group prevents terminal Ctrl-C from reaching the helper while avirthostd receives it.
            .process_group(0)
            .spawn()
            .with_context(|| format!("spawn {}", spec.bin.display()))?;
        let stdin = child.stdin.take().context("avirtvmd's stdin")?;
        Ok((child, stdin))
    }

    /// The state of a helper that holds its lock: from its connection if this supervisor started
    /// it, and from a probe otherwise.
    async fn live_state(&self, name: &str, dir: &Path, intent: &Intent) -> HelperState {
        let ours = self.records().get(name).map(|record| record.handle.is_closed());
        match ours {
            Some(false) => HelperState::Running,
            Some(true) => HelperState::Unknown("the control connection is closed".to_owned()),
            None => self.probe(dir, intent).await,
        }
    }

    /// A supervisor for the helpers in `workloads_dir`.
    #[must_use]
    pub fn new(workloads_dir: PathBuf) -> Self {
        Self { probe_timeout: PROBE_TIMEOUT, records: Mutex::new(HashMap::new()), workloads_dir }
    }

    /// Classifies a live helper that this supervisor didn't start.
    async fn probe(&self, dir: &Path, intent: &Intent) -> HelperState {
        match Self::reattach(dir, intent, self.probe_timeout).await {
            Ok(_) => HelperState::Running,
            Err(err) => match err.downcast_ref::<HandshakeError>() {
                Some(HandshakeError::UnsupportedVersion { .. }) => HelperState::Incompatible(err.to_string()),
                Some(_) | None => HelperState::Unknown(err.to_string()),
            },
        }
    }

    /// Connects to a helper that this supervisor didn't start, using its launch intent.
    ///
    /// # Errors
    ///
    /// Returns an error if the intent has a bad incarnation or the handshake fails.
    async fn reattach(dir: &Path, intent: &Intent, limit: Duration) -> Result<Client> {
        let incarnation = Incarnation::from_hex(&intent.incarnation).context("the launch intent's incarnation")?;
        let expected = Expected { incarnation: &incarnation, workload: &intent.workload };
        time::timeout(limit, control::connect(&dir.join("v.sock"), &expected, limit))
            .await
            .map_err(|_| HandshakeError::NoHello)?
            .map_err(Into::into)
    }

    /// Rewrites the intent in `dir` with the state and pid after a spawn.
    ///
    /// # Errors
    ///
    /// Returns an error if reading or writing the intent fails.
    async fn record_spawn(dir: &Path, pid: Option<u32>) -> Result<()> {
        let path = dir.to_path_buf();
        task::spawn_blocking(move || {
            let mut intent = Intent::read(&path)?;
            intent.state = State::Starting;
            intent.pid = pid;
            intent.write(&path)
        })
        .await?
        .context("record the helper's pid in its launch intent")
    }

    /// The helpers this supervisor started.
    fn records(&self) -> MutexGuard<'_, HashMap<String, Record>> {
        self.records.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// What avirthostd knows about the helper in `dir`, or nothing if no helper ever spawned for it.
    async fn status_of(&self, dir: &Path, name: &str) -> Option<HelperStatus> {
        let intent = match Intent::read(dir) {
            Ok(intent) => intent,
            // A directory with no intent never had a helper spawned for it.
            Err(err) if err.kind() == io::ErrorKind::NotFound => return None,
            Err(err) => {
                let state = HelperState::Unknown(format!("the launch intent is unreadable: {err}"));
                return Some(HelperStatus { incarnation: None, recorded: None, state, workload: name.to_owned() });
            }
        };
        let recorded_ended = intent.state == State::Ended;
        let state = match lock_state(dir) {
            // avirthostd saw the lock free before it recorded the end, and no helper spawns again for
            // this directory. The record answers when the lock check fails.
            LockState::Failed(_) | LockState::Free | LockState::Missing if recorded_ended => HelperState::Ended,
            LockState::Failed(why) => HelperState::Unknown(format!("the helper's lock: {why}")),
            LockState::Free | LockState::Missing => HelperState::Ended,
            LockState::Held if recorded_ended => {
                HelperState::Unknown("the launch intent records that the helper ended, but its lock is held".to_owned())
            }
            LockState::Held => self.live_state(name, dir, &intent).await,
        };
        let incarnation = Incarnation::from_hex(&intent.incarnation);
        Some(HelperStatus { incarnation, recorded: Some(intent.state), state, workload: name.to_owned() })
    }

    /// The connection for asking a helper to stop: this supervisor's own if it started the
    /// helper, and a fresh one if a live helper holds the lock.
    async fn stop_connection(&self, record: Option<&Record>, intent: Option<&Intent>, dir: &Path) -> Option<Client> {
        match (record, intent) {
            (Some(started), _) => Some(started.handle.0.clone()),
            (None, Some(launch)) if lock_state(dir) == LockState::Held => {
                Self::reattach(dir, launch, self.probe_timeout).await.ok()
            }
            (None, Some(_) | None) => None,
        }
    }

    /// Waits until nothing holds the helper's lock, or `limit` passes. Returns whether it's free.
    async fn wait_for_free_lock(dir: &Path, limit: Duration) -> bool {
        let started = Instant::now();
        while lock_state(dir) == LockState::Held {
            if started.elapsed() >= limit {
                return false;
            }
            time::sleep(Duration::from_millis(100)).await;
        }
        true
    }
}

#[async_trait]
impl HelperSupervisor for ChildSupervisor {
    async fn list(&self) -> Result<Vec<HelperStatus>> {
        let mut statuses = Vec::new();
        let mut entries = fs::read_dir(&self.workloads_dir)
            .await
            .with_context(|| format!("list {}", self.workloads_dir.display()))?;
        while let Some(entry) = entries.next_entry().await? {
            // Only directories can hold a helper. A symlink isn't followed.
            if !entry.file_type().await?.is_dir() {
                continue;
            }
            statuses.extend(self.status_of(&entry.path(), &entry.file_name().to_string_lossy()).await);
        }
        Ok(statuses)
    }

    async fn start(&self, workload: &str, incarnation: &Incarnation, spec: &HelperSpec) -> Result<HelperHandle> {
        if let Some(record) = self.records().get(workload) {
            if record.incarnation == *incarnation {
                return Ok(record.handle.clone());
            }
            bail!("workload {workload} already has a helper with another incarnation");
        }
        let (mut child, mut stdin) = Self::launch(workload, spec)?;
        let expected = Expected { incarnation, workload };
        let handle = HelperHandle(Self::handshake(&mut child, &mut stdin, &expected, spec).await?);
        let record = Record { _stdin: stdin, child, handle: handle.clone(), incarnation: *incarnation };
        self.records().insert(workload.to_owned(), record);
        Ok(handle)
    }

    async fn stop(&self, workload: &str, grace: Duration) -> Result<()> {
        let dir = self.workloads_dir.join(workload);
        let mut record = self.records().remove(workload);
        let intent = match Intent::read(&dir) {
            Ok(intent) => Some(intent),
            Err(err) if err.kind() == io::ErrorKind::NotFound => None,
            Err(err) => return Err(err).with_context(|| format!("read the launch intent of workload {workload}")),
        };
        let ended = intent.as_ref().is_some_and(|launch| launch.state == State::Ended);
        if ended && lock_state(&dir) != LockState::Held {
            return Ok(());
        }
        // A failed write doesn't hold up the stop: stopping the VM is a safety action, and a restart
        // stops a `running` workload the same way.
        let before_stop = intent
            .as_ref()
            .is_some_and(|launch| matches!(launch.state, State::Intended | State::Starting | State::Running));
        let marked =
            if before_stop { rewrite_intent(&dir, |launch| launch.state = State::Stopping).await } else { Ok(()) };
        if let Some(client) = self.stop_connection(record.as_ref(), intent.as_ref(), &dir).await {
            request_stop(&client, grace).await;
        }
        wait_for_exit(record.as_mut(), &dir, grace.saturating_add(EXIT_SLACK)).await;
        force_exit(record.as_ref(), intent.as_ref(), &dir).await;
        let outcome = match lock_state(&dir) {
            LockState::Free | LockState::Missing => {
                if intent.is_some() {
                    rewrite_intent(&dir, |launch| {
                        launch.state = State::Ended;
                        launch.pid = None;
                    })
                    .await
                    .with_context(|| format!("record that the helper of workload {workload} ended"))?;
                }
                return Ok(());
            }
            LockState::Held => {
                if let Some(held) = record {
                    self.records().insert(workload.to_owned(), held);
                }
                anyhow!("avirtvmd for workload {workload} is still running")
            }
            LockState::Failed(why) => anyhow!("cannot tell whether avirtvmd for workload {workload} is gone: {why}"),
        };
        match marked {
            Ok(()) => Err(outcome),
            Err(err) => Err(outcome.context(format!("the launch intent was not rewritten to stopping: {err}"))),
        }
    }
}

/// Rewrites the intent in `dir` with `change`.
///
/// # Errors
///
/// Returns an error if reading or writing the intent fails.
async fn rewrite_intent(dir: &Path, change: impl FnOnce(&mut Intent) + Send + 'static) -> io::Result<()> {
    let path = dir.to_path_buf();
    task::spawn_blocking(move || {
        let mut intent = Intent::read(&path)?;
        change(&mut intent);
        intent.write(&path)
    })
    .await
    .map_err(io::Error::other)?
}

/// Asks the helper to stop its VM and exit.
async fn request_stop(client: &Client, grace: Duration) {
    let stop = RequestOp::Stop(Stop { grace_seconds: u32::try_from(grace.as_secs()).unwrap_or(u32::MAX) });
    drop(time::timeout(EXIT_SLACK, client.request(stop)).await);
}

/// The pid to send SIGKILL, from the child this supervisor started or the launch intent.
fn kill_target(record: Option<&Record>, intent: Option<&Intent>) -> Option<Pid> {
    let raw = record.and_then(|started| started.child.id()).or_else(|| intent?.pid)?;
    i32::try_from(raw).ok().and_then(Pid::from_raw)
}

/// Sends SIGKILL to a helper that still holds its lock after the grace period.
///
/// Only a held lock proves that the recorded helper is the one alive.
async fn force_exit(record: Option<&Record>, intent: Option<&Intent>, dir: &Path) {
    if lock_state(dir) != LockState::Held {
        return;
    }
    if let Some(pid) = kill_target(record, intent) {
        // The helper may have exited since the lock check, which leaves nothing to signal.
        match process::kill_process(pid, Signal::KILL) {
            Ok(()) | Err(_) => {}
        }
    }
    ChildSupervisor::wait_for_free_lock(dir, KILL_WAIT).await;
}

/// What `<dir>/vmd.lock` says. A process's death releases the lock, so a free lock confirms that
/// the helper and its VM have exited.
fn lock_state(dir: &Path) -> LockState {
    let file = match File::open(dir.join("vmd.lock")) {
        Ok(file) => file,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return LockState::Missing,
        Err(err) => return LockState::Failed(err.to_string()),
    };
    // Taking the lock succeeds only when no helper holds it. Closing the file releases it again.
    match file.try_lock() {
        Ok(()) => LockState::Free,
        Err(TryLockError::WouldBlock) => LockState::Held,
        Err(TryLockError::Error(err)) => LockState::Failed(err.to_string()),
    }
}

/// Waits for the helper to exit: on the child if this supervisor started it, and on the lock
/// otherwise.
async fn wait_for_exit(record: Option<&mut Record>, dir: &Path, limit: Duration) {
    match record {
        Some(started) => drop(time::timeout(limit, started.child.wait()).await),
        None => drop(ChildSupervisor::wait_for_free_lock(dir, limit).await),
    }
}
