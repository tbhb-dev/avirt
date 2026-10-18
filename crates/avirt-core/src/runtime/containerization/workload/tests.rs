//! Unit tests for the teardown paths of a workload handle, against a fake helper behind a stand-in
//! process.

use super::{Containerization, Handle};
use crate::egress::WorkloadDir;
use crate::runtime::containerization::intent::{Intent, State};
use crate::runtime::containerization::lifecycle::{ChildSupervisor, Incarnation};
use crate::runtime::containerization::proto::StartWorkload;
use crate::runtime::containerization::testing::{FakeHelper, hello};
use crate::runtime::{MediatorSpec, Runtime as _, WorkloadHandle as _, WorkloadSpec};
use crate::state::Root;
use alloc::sync::Arc;
use anyhow::{Context as _, Result, bail, ensure};
use core::time::Duration;
use rustix::process::{Pid, test_kill_process};
use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use tokio::time::{self, Instant};

/// The workload id the tests use.
const ID: &str = "1a2b3c4d";

/// A handle over directories under `base`, with a stand-in helper process that only sleeps.
struct Fixture {
    /// The VM state directory.
    container_dir: PathBuf,
    /// The handle under test.
    handle: Handle,
    /// The workload directory.
    workload_dir: PathBuf,
}

/// A temporary directory with a short path, because the sockets under it must fit `sun_path`.
///
/// # Errors
///
/// Fails when creating the directory fails.
fn short_dir() -> Result<tempfile::TempDir> {
    Ok(tempfile::Builder::new().prefix("w").tempdir_in("/tmp")?)
}

/// Writes an executable that ignores its arguments and sleeps, in place of avirtvmd.
///
/// # Errors
///
/// Fails when writing the script fails.
fn sleeper(base: &Path) -> Result<PathBuf> {
    let path = base.join("avirtvmd");
    fs::write(&path, "#!/bin/sh\nexec sleep 60\n")?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

/// Writes an executable that listens on the two workload sockets, in place of avirtmediatord.
///
/// # Errors
///
/// Fails when writing the script fails.
fn listener(base: &Path) -> Result<PathBuf> {
    let path = base.join("mediator");
    let script = "#!/usr/bin/env python3\nimport os, socket, sys, time\n\
        held = []\n\
        d = sys.argv[sys.argv.index('--dir') + 1]\n\
        for n in ('proxy.sock', 'mediator.sock'):\n\
        \ts = socket.socket(socket.AF_UNIX); s.bind(os.path.join(d, n)); s.listen(8); held.append(s)\n\
        time.sleep(60)\n";
    fs::write(&path, script)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

/// Starts the handle's mediator stand-in.
///
/// # Errors
///
/// Fails when the stand-in doesn't open its sockets.
async fn with_mediator(fixture: &mut Fixture, base: &Path) -> Result<()> {
    let spec = MediatorSpec {
        bin: listener(base)?,
        hostd_sock: base.join("hostd.sock"),
        log: base.join("mediator.log"),
        secret: "secret".to_owned(),
    };
    fixture.handle.start_mediator(spec).await
}

/// Builds the directories, the launch intent, and a handle for workload `ID`.
///
/// # Errors
///
/// Fails when setting up the directories fails.
fn fixture(base: &Path) -> Result<Fixture> {
    let state = Root::open_in(base)?;
    let dir = WorkloadDir::create(&state.workloads(), ID)?;
    let workload_dir = state.workloads().join(ID);
    let container_dir = state.containers().join(ID);
    fs::create_dir_all(&container_dir)?;
    let logs_dir = base.join("logs");
    fs::create_dir_all(&logs_dir)?;
    let incarnation = Incarnation::random();
    let intent = Intent {
        incarnation: incarnation.to_hex(),
        pid: None,
        spec_digest: "ab".repeat(32),
        state: State::Intended,
        workload: ID.to_owned(),
    };
    intent.write(&workload_dir)?;
    let spec = WorkloadSpec {
        dir,
        env: Vec::new(),
        id: ID.to_owned(),
        image: "image:dev".to_owned(),
        logs_dir,
        shares: Vec::new(),
    };
    let handle = Handle {
        client: None,
        containers: state.containers(),
        incarnation,
        proxy: None,
        request: StartWorkload::default(),
        spec,
        start_timeout: Duration::from_mins(1),
        supervisor: Arc::new(ChildSupervisor::new(state.workloads())),
        vmd_bin: sleeper(base)?,
    };
    Ok(Fixture { container_dir, handle, workload_dir })
}

/// Whether the process `pid` still exists.
fn alive(pid: u32) -> bool {
    i32::try_from(pid).ok().and_then(Pid::from_raw).is_some_and(|target| test_kill_process(target).is_ok())
}

/// Waits for the process `pid` to exit.
///
/// # Errors
///
/// Fails when the process still runs after the wait.
async fn exited(pid: u32) -> Result<()> {
    for _ in 0u8..100u8 {
        if !alive(pid) {
            return Ok(());
        }
        time::sleep(Duration::from_millis(50)).await;
    }
    bail!("process {pid} still runs")
}

/// A refused start ends with the helper stopped and both directories removed.
///
/// # Errors
///
/// Fails when the start succeeds, the helper keeps running, or a directory remains.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_failure_reply_to_the_start_ends_in_a_full_teardown() -> Result<()> {
    let base = short_dir()?;
    let mut fixture = fixture(base.path())?;
    let greeting = hello(ID, &fixture.handle.incarnation, 1, 1);
    with_mediator(&mut fixture, base.path()).await?;
    let _fake = FakeHelper::spawn_refusing_starts(&fixture.workload_dir.join("v.sock"), Some(greeting))?;

    let Err(err) = fixture.handle.start_workload().await else { bail!("the start succeeded") };
    ensure!(err.to_string().contains("the VM can't boot"), "the error is {err:#}");
    let pid = Intent::read(&fixture.workload_dir)?.pid.context("the intent has no pid")?;
    ensure!(alive(pid), "the stand-in helper isn't running");

    fixture.handle.stop_mediator().await;
    Box::new(fixture.handle).release(true).await;
    exited(pid).await?;
    ensure!(!fixture.workload_dir.exists(), "the workload directory remains");
    ensure!(!fixture.container_dir.exists(), "the VM state remains");
    Ok(())
}

/// The start fails after the timeout when the helper doesn't answer the start request.
///
/// # Errors
///
/// Fails when the start succeeds or outlasts the timeout, or when the error names another cause.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_helper_that_never_answers_the_start_request_times_out() -> Result<()> {
    let base = short_dir()?;
    let mut fixture = fixture(base.path())?;
    fixture.handle.start_timeout = Duration::from_millis(300);
    with_mediator(&mut fixture, base.path()).await?;
    let greeting = hello(ID, &fixture.handle.incarnation, 1, 1);
    let _silent = FakeHelper::spawn_ignoring_starts(&fixture.workload_dir.join("v.sock"), Some(greeting))?;

    let began = Instant::now();
    let Err(err) = fixture.handle.start_workload().await else { bail!("the start succeeded") };
    ensure!(err.to_string().contains("didn't answer the start request"), "the error is {err:#}");
    ensure!(began.elapsed() < Duration::from_secs(10), "the start waited {:?}", began.elapsed());
    fixture.handle.stop_mediator().await;
    Box::new(fixture.handle).release(true).await;
    Ok(())
}

/// The supervisor kills a helper that binds its socket and never completes the handshake.
///
/// # Errors
///
/// Fails when the start succeeds or the helper keeps running.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_helper_that_fails_the_handshake_is_killed() -> Result<()> {
    let base = short_dir()?;
    let mut fixture = fixture(base.path())?;
    with_mediator(&mut fixture, base.path()).await?;
    let _silent = FakeHelper::spawn(&fixture.workload_dir.join("v.sock"), None)?;

    ensure!(fixture.handle.start_workload().await.is_err(), "the start succeeded");
    let pid = Intent::read(&fixture.workload_dir)?.pid.context("the intent has no pid")?;
    exited(pid).await?;

    fixture.handle.stop_mediator().await;
    Box::new(fixture.handle).release(true).await;
    ensure!(!fixture.workload_dir.is_dir(), "release left the workload directory");
    ensure!(!fixture.container_dir.is_dir(), "release left the VM state");
    Ok(())
}

/// The startup sweep removes temporary entries from the caches and the image store, and keeps the rest.
///
/// # Errors
///
/// Fails when a stale entry remains, a finished entry goes, or the sweep reports a failure.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn the_startup_sweep_removes_stale_temporary_entries() -> Result<()> {
    let base = short_dir()?;
    let state = Root::open_in(base.path())?;
    let blobs = state.images().join("blobs").join("sha256");
    let leftovers = [
        state.cache().join("kernel").join(".tmp-0000000000000001"),
        state.cache().join("initfs").join(".tmp-0000000000000002"),
        state.cache().join("rootfs").join(".tmp-0000000000000003"),
        blobs.join(".tmp-0000000000000004"),
    ];
    for path in &leftovers {
        fs::create_dir_all(path)?;
    }
    let finished = state.cache().join("kernel").join("abc");
    fs::create_dir_all(&finished)?;
    let runtime = Containerization::new(&state, base.path().join("avirtvmd"), base.path().join("archives"));
    let swept = runtime.sweep(&state).await;
    ensure!(swept.failures.is_empty(), "the sweep reported {:?}", swept.failures);
    ensure!(leftovers.iter().all(|path| !path.exists()), "a temporary entry remains");
    ensure!(finished.is_dir(), "the sweep removed a finished entry");
    Ok(())
}
