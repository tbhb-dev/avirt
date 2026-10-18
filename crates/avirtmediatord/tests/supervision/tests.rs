//! The supervision checks and the fake avirthostd they run against.

use core::sync::atomic::{AtomicU64, Ordering};
use core::time::Duration;
use std::env;
use std::fs;
use std::io::{self, BufRead as _, BufReader};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::process::{self, Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{Sender, channel};
use std::thread::{self, sleep};
use std::time::Instant;

use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;

/// How long any wait on a subprocess may take.
const BOUND: Duration = Duration::from_secs(5);

/// A running mediator with a fake avirthostd listening for its events.
struct Harness {
    /// The scratch directory the mediator runs in.
    dir: Scratch,
    /// The mediator process.
    mediator: Process,
}

/// A child process that dies when dropped.
struct Process(Child);

/// A scratch directory, as a path relative to the working directory, removed when dropped.
struct Scratch(PathBuf);

impl Harness {
    /// Starts a mediator against a fake avirthostd.
    ///
    /// # Errors
    ///
    /// Fails if the mediator doesn't start or doesn't signal that it's ready in time.
    fn start() -> io::Result<Self> {
        let dir = Scratch::new()?;
        let socket = dir.0.join("h");
        let listener = UnixListener::bind(&socket)?;
        listener.set_nonblocking(true)?;
        let (sender, lines) = channel();
        thread::spawn(move || serve_events(&listener, &sender));
        let mediator = Process(
            Command::new(env!("CARGO_BIN_EXE_avirtmediatord"))
                .arg("--dir")
                .arg(&dir.0)
                .arg("--hostd-sock")
                .arg(&socket)
                .args(["--workload", "test"])
                .env("AVIRT_MEDIATOR_SECRET", "test-secret")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()?,
        );
        let hello = lines.recv_timeout(BOUND).map_err(io::Error::other)?;
        ensure(hello == "EGRESS test test-secret", "the mediator greeted avirthostd with an unexpected line")?;
        let harness = Self { dir, mediator };
        until(|| harness.dir.0.join("proxy.sock").exists())?;
        Ok(harness)
    }
}

impl Process {
    /// Kills the process and waits, within the bound, for it to exit.
    fn stop(&mut self) {
        self.0.kill().unwrap_or_default();
        let started = Instant::now();
        while matches!(self.0.try_wait(), Ok(None)) && started.elapsed() < BOUND {
            sleep(Duration::from_millis(20));
        }
    }

    /// Waits for the process to exit and returns its status.
    ///
    /// # Errors
    ///
    /// Fails if the process is still running after the bound.
    fn wait(&mut self) -> io::Result<ExitStatus> {
        let mut status = None;
        until(|| {
            status = self.0.try_wait().ok().flatten();
            status.is_some()
        })?;
        status.ok_or_else(|| io::Error::other("the process has no exit status"))
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Stops the mediator before the field drops remove the scratch directory it runs in.
impl Drop for Harness {
    fn drop(&mut self) {
        self.mediator.stop();
    }
}

impl Scratch {
    /// Creates a fresh directory under the target directory.
    ///
    /// # Errors
    ///
    /// Fails when creating the directory fails.
    fn new() -> io::Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let target = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
        fs::create_dir_all(&target)?;
        let pid = process::id();
        let serial = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = target.join(format!("s{pid}-{serial}"));
        fs::create_dir_all(&dir)?;
        // Relative Unix socket names avoid macOS's sockaddr_un path-length limit.
        let cwd = env::current_dir()?;
        let common = cwd.components().zip(dir.components()).take_while(|(left, right)| left == right).count();
        let mut relative = PathBuf::new();
        for _ in cwd.components().skip(common) {
            relative.push("..");
        }
        relative.extend(dir.components().skip(common));
        Ok(Self(relative))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap_or_default();
    }
}

/// With the Pingora engine, SIGTERM stops the mediator even while stdin is open.
///
/// # Errors
///
/// Fails if the mediator exits with a failure or a subprocess doesn't respond in time.
#[test]
fn pingora_sigterm_exits_with_stdin_open() -> io::Result<()> {
    let mut harness = Harness::start()?;
    let pid = i32::try_from(harness.mediator.0.id()).map_err(io::Error::other)?;
    kill(Pid::from_raw(pid), Signal::SIGTERM)?;
    ensure(harness.mediator.wait()?.success(), "the mediator exited with a failure")?;
    ensure(harness.mediator.0.stdin.is_some(), "the mediator's stdin pipe is gone")?;
    Ok(())
}

/// Accepts avirthostd's event connection and forwards its lines to `sender`.
fn serve_events(listener: &UnixListener, sender: &Sender<String>) {
    let mut accepted = None;
    if until(|| {
        accepted = listener.accept().ok();
        accepted.is_some()
    })
    .is_err()
    {
        return;
    }
    let Some((stream, _)) = accepted else { return };
    if stream.set_nonblocking(false).is_err() || stream.set_read_timeout(Some(BOUND)).is_err() {
        return;
    }
    for line in BufReader::new(stream).lines().map_while(Result::ok) {
        if sender.send(line).is_err() {
            break;
        }
    }
}

/// Fails with `message` unless `condition` holds.
///
/// # Errors
///
/// Fails when `condition` is false.
fn ensure(condition: bool, message: &str) -> io::Result<()> {
    if condition { Ok(()) } else { Err(io::Error::other(message.to_owned())) }
}

/// Polls `ready` until it returns true.
///
/// # Errors
///
/// Fails if `ready` is still false after the bound.
fn until(mut ready: impl FnMut() -> bool) -> io::Result<()> {
    let started = Instant::now();
    while !ready() {
        if started.elapsed() >= BOUND {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "timed out waiting for subprocess"));
        }
        sleep(Duration::from_millis(20));
    }
    Ok(())
}
