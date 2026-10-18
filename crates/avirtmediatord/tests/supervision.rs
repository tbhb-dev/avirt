//! Supervision checks with a fake Envoy and avirthostd, without actors or workloads.
use std::fs;
use std::io::{BufRead, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;
use serde_json::Value;

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

    fn pid(&self, name: &str) -> i32 {
        let path = self.0.join(name);
        let mut pid = None;
        until(|| {
            pid = fs::read_to_string(&path).ok().and_then(|s| s.trim().parse().ok());
            pid.is_some()
        });
        pid.unwrap()
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
    lines: Receiver<String>,
    dir: Scratch,
}
impl Harness {
    fn start(envoy: bool) -> Self {
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
        let fake = dir.0.join("fake-envoy");
        fs::write(&fake, FAKE).unwrap();
        fs::set_permissions(&fake, fs::Permissions::from_mode(0o700)).unwrap();
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_avirtmediatord"));
        cmd.arg("--dir")
            .arg(&dir.0)
            .arg("--hostd-sock")
            .arg(&socket)
            .args(["--actor", "test"])
            .env("AVIRT_MEDIATOR_SECRET", "test-secret")
            .env_remove("AVIRT_PROXY_ENGINE")
            .env_remove("AVIRT_ENVOY_BIN")
            .env_remove("AVIRT_ENVOY_MODULE")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit());
        if envoy {
            cmd.args(["--proxy-engine", "envoy", "--envoy-bin"]).arg(&fake).arg("--envoy-module").arg(&fake);
        }
        let mediator = Process(cmd.spawn().unwrap());
        assert_eq!(lines.recv_timeout(BOUND).unwrap(), "EGRESS test test-secret");
        let harness = Self { mediator, lines, dir };
        until(|| harness.dir.0.join("proxy.sock").exists());
        harness
    }
}

// Envoy::start's first connection is a readiness probe. Subsequent accepts model a stream.
const FAKE: &str = r#"#!/bin/sh
exec /usr/bin/perl -e '
use strict;
use warnings;
use IO::Socket::UNIX;
use Socket qw(SOCK_STREAM);
my $dir = shift;
$| = 1;
my $listener = IO::Socket::UNIX->new(Type => SOCK_STREAM, Local => "$dir/envoy.sock", Listen => 8) or die $!;
open(my $pid, ">", "$dir/envoy.pid") or die $!;
print $pid "$$\n"; close($pid);
open(my $parent, ">", "$dir/guardian.pid") or die $!;
print $parent getppid(), "\n"; close($parent);
my $probe = $listener->accept() or die $!;
close($probe);
while (my $conn = $listener->accept()) {
    print "{\"avirt_conn\":1,\"complete\":false,\"event\":{\"event\":\"conn\",\"decision\":\"allow\",\"rule\":\"allow-all\",\"error\":null}}\n";
    open(my $marker, ">", "$dir/accepted") or die $!;
    close($marker);
    my $buf;
    while (sysread($conn, $buf, 1024)) {}
    print "{\"avirt_conn\":1,\"complete\":true,\"event\":{\"event\":\"conn\",\"decision\":\"allow\",\"rule\":\"allow-all\",\"error\":null}}\n";
    close($conn);
}
' "$(dirname "$5")"
"#;

#[test]
fn guardian_reaps_envoy_when_parent_pipe_closes() {
    let dir = Scratch::new();
    let fake = dir.0.join("fake-envoy");
    fs::write(&fake, "#!/bin/sh\nexec /usr/bin/perl -e 'open(my $p, \">\", \"child.pid\") or die $!; print $p \"$$\\n\"; close($p); sleep 60;'\n").unwrap();
    fs::set_permissions(&fake, fs::Permissions::from_mode(0o700)).unwrap();
    let mut guardian = Process(
        Command::new(env!("CARGO_BIN_EXE_avirtmediatord"))
            .arg("--supervise-envoy")
            .arg(fake.canonicalize().unwrap())
            .arg("unused-config")
            .current_dir(&dir.0)
            .stdin(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let pid = dir.pid("child.pid");
    drop(guardian.0.stdin.take());
    guardian.wait();
    until(|| kill(Pid::from_raw(pid), None).is_err());
}

#[test]
fn envoy_exit_shuts_down_mediator() {
    let mut h = Harness::start(true);
    kill(Pid::from_raw(h.dir.pid("envoy.pid")), Signal::SIGKILL).unwrap();
    assert!(!h.mediator.wait().success());
    assert!(h.mediator.0.stdin.is_some());
}

#[test]
fn guardian_sigterm_does_not_orphan_envoy() {
    let mut h = Harness::start(true);
    let envoy = h.dir.pid("envoy.pid");
    kill(Pid::from_raw(h.dir.pid("guardian.pid")), Signal::SIGTERM).unwrap();
    assert!(!h.mediator.wait().success());
    until(|| kill(Pid::from_raw(envoy), None).is_err());
}

fn pgid(pid: u32) -> String {
    let output = Command::new("ps").args(["-o", "pgid=", "-p", &pid.to_string()]).output().unwrap();
    assert!(output.status.success());
    let group = String::from_utf8(output.stdout).unwrap().trim().to_owned();
    assert!(!group.is_empty());
    group
}

#[test]
fn envoy_runs_in_its_own_process_group() {
    let mut h = Harness::start(true);
    assert_ne!(pgid(u32::try_from(h.dir.pid("envoy.pid")).unwrap()), pgid(h.mediator.0.id()));
    drop(h.mediator.0.stdin.take());
    assert!(h.mediator.wait().success());
}

#[test]
fn open_stream_is_flushed_with_shutdown() {
    let mut h = Harness::start(true);
    let _stream = UnixStream::connect(h.dir.0.join("proxy.sock")).unwrap();
    until(|| h.dir.0.join("accepted").exists());
    drop(h.mediator.0.stdin.take());
    assert!(h.mediator.wait().success());
    let mut lines = Vec::new();
    loop {
        match h.lines.recv_timeout(BOUND) {
            Ok(line) => lines.push(line),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            Err(error) => panic!("event reader did not finish: {error}"),
        }
    }
    let events: Vec<Value> = lines.iter().map(|s| serde_json::from_str(s).unwrap()).collect();
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0]["event"], "conn");
    assert_eq!(events[0]["error"], "shutdown");
}

#[test]
fn pingora_sigterm_exits_with_stdin_open() {
    let mut h = Harness::start(false);
    kill(Pid::from_raw(i32::try_from(h.mediator.0.id()).unwrap()), Signal::SIGTERM).unwrap();
    assert!(h.mediator.wait().success());
    assert!(h.mediator.0.stdin.is_some());
}
