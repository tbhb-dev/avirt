//! Exercises the guardian without an actor, a workload, or an installed Envoy.
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn guardian_reaps_envoy_when_parent_pipe_closes() {
    let target = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target");
    fs::create_dir_all(&target).unwrap();
    let dir = Scratch(target.join(format!("guardian-test-{}", std::process::id())));
    fs::create_dir(&dir.0).unwrap();
    let fake = dir.0.join("fake-envoy");
    // The cwd is controlled by the test, so the script needs no interpolated paths.
    fs::write(&fake, "#!/bin/sh\necho $$ > child.pid\nexec sleep 60\n").unwrap();
    fs::set_permissions(&fake, fs::Permissions::from_mode(0o700)).unwrap();
    let mut guardian = Command::new(env!("CARGO_BIN_EXE_avirtmediatord"))
        .arg("--supervise-envoy")
        .arg(&fake)
        .arg("unused-config")
        .current_dir(&dir.0)
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    let pid = loop {
        match fs::read_to_string(dir.0.join("child.pid")) {
            Ok(pid) if !pid.trim().is_empty() => break pid.trim().to_owned(),
            Err(error) if error.kind() != io::ErrorKind::NotFound => panic!("{error}"),
            _ => {}
        }
        assert!(Instant::now() < deadline, "fake Envoy did not start");
        std::thread::sleep(Duration::from_millis(20));
    };
    drop(guardian.stdin.take());
    loop {
        if guardian.try_wait().unwrap().is_some() {
            break;
        }
        assert!(Instant::now() < deadline, "guardian did not exit");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(!Command::new("kill").args(["-0", &pid]).stderr(Stdio::null()).status().unwrap().success());
}
