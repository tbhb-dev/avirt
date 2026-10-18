//! Supervision checks use a fake avirthostd. They don't launch workloads.
use std::fs;
use std::io::{BufRead, BufReader};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;

const BOUND: Duration = Duration::from_secs(5);

fn until(mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + BOUND;
    while !ready() {
        assert!(Instant::now() < deadline, "timed out waiting for subprocess");
        std::thread::sleep(Duration::from_millis(20));
    }
}

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let target = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
        fs::create_dir_all(&target).unwrap();
        let dir = target.join(format!("s{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        fs::create_dir(&dir).unwrap();
        // Relative Unix socket names avoid macOS's sockaddr_un path-length limit.
        let cwd = std::env::current_dir().unwrap();
        let common = cwd.components().zip(dir.components()).take_while(|(a, b)| a == b).count();
        let mut relative = PathBuf::new();
        for _ in cwd.components().skip(common) {
            relative.push("..");
        }
        relative.extend(dir.components().skip(common));
        Self(relative)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Process(Child);
impl Process {
    fn wait(&mut self) -> ExitStatus {
        let mut status = None;
        until(|| {
            status = self.0.try_wait().unwrap();
            status.is_some()
        });
        status.unwrap()
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let deadline = Instant::now() + BOUND;
        while matches!(self.0.try_wait(), Ok(None)) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

struct Harness {
    mediator: Process,
    dir: Scratch,
}
impl Harness {
    fn start() -> Self {
        let dir = Scratch::new();
        let socket = dir.0.join("h");
        let listener = UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();
        let (tx, lines) = channel();
        std::thread::spawn(move || {
            let mut accepted = None;
            until(|| {
                accepted = listener.accept().ok();
                accepted.is_some()
            });
            let (stream, _) = accepted.unwrap();
            stream.set_nonblocking(false).unwrap();
            stream.set_read_timeout(Some(BOUND)).unwrap();
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_avirtmediatord"));
        cmd.arg("--dir")
            .arg(&dir.0)
            .arg("--hostd-sock")
            .arg(&socket)
            .args(["--workload", "test"])
            .env("AVIRT_MEDIATOR_SECRET", "test-secret")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit());
        let mediator = Process(cmd.spawn().unwrap());
        assert_eq!(lines.recv_timeout(BOUND).unwrap(), "EGRESS test test-secret");
        let harness = Self { mediator, dir };
        until(|| harness.dir.0.join("proxy.sock").exists());
        harness
    }
}

#[test]
fn pingora_sigterm_exits_with_stdin_open() {
    let mut h = Harness::start();
    kill(Pid::from_raw(i32::try_from(h.mediator.0.id()).unwrap()), Signal::SIGTERM).unwrap();
    assert!(h.mediator.wait().success());
    assert!(h.mediator.0.stdin.is_some());
}
