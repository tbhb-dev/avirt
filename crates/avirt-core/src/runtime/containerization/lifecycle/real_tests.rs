//! Tests that start a real avirtvmd, behind the `avirtvmd-tests` feature.
//!
//! Build it with `mise run build-vmd` and set `AVIRT_VMD_BIN` to `target/debug/avirtvmd`.
//! `mise run check-vmd` does both.

#![cfg(feature = "avirtvmd-tests")]

use super::super::intent::{Intent, State};
use super::super::proto::{RequestOp, ResponseResult, State as WorkloadState, Status};
use super::{
    ChildSupervisor, HelperHandle, HelperSpec, HelperState, HelperSupervisor as _, Incarnation, LockState, Record,
    lock_state,
};
use anyhow::{Context as _, Result, bail, ensure};
use core::time::Duration;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use tokio::time;

/// The built helper.
///
/// # Errors
///
/// Fails when `AVIRT_VMD_BIN` isn't set.
fn bin() -> Result<PathBuf> {
    env::var_os("AVIRT_VMD_BIN").map(PathBuf::from).context("set AVIRT_VMD_BIN to the built avirtvmd")
}

/// A workload directory under `root` with its launch intent, and the spec to start a helper in it.
///
/// # Errors
///
/// Fails when writing the directory or the intent fails.
fn workload(root: &Path, id: &str, incarnation: Incarnation) -> Result<HelperSpec> {
    let dir = root.join(id);
    fs::create_dir_all(&dir)?;
    let intent = Intent {
        incarnation: incarnation.to_hex(),
        pid: None,
        spec_digest: "ab".repeat(32),
        state: State::Intended,
        workload: id.to_owned(),
    };
    intent.write(&dir)?;
    Ok(HelperSpec { bin: bin()?, dir, log: root.join(format!("{id}.avirtvmd.log")) })
}

/// The helper's reported state.
///
/// # Errors
///
/// Fails when the request fails or the answer isn't a status.
async fn state_of(handle: &HelperHandle) -> Result<WorkloadState> {
    match handle.request(RequestOp::Status(Status)).await? {
        ResponseResult::Status(reply) => Ok(WorkloadState::try_from(reply.state)?),
        other @ (ResponseResult::Failure(_) | ResponseResult::Started(_) | ResponseResult::Stopped(_)) => {
            bail!("expected a status, got {other:?}")
        }
    }
}

/// Starting twice with one incarnation returns the one helper.
///
/// # Errors
///
/// Fails when the second start differs from the first.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn starting_twice_with_one_incarnation_returns_the_one_helper() -> Result<()> {
    let root = tempfile::tempdir()?;
    let incarnation = Incarnation::random();
    let spec = workload(root.path(), "1a2b3c4d", incarnation)?;
    let supervised = ChildSupervisor::new(root.path().to_path_buf());
    let first = supervised.start("1a2b3c4d", &incarnation, &spec).await?;
    let pid = Intent::read(&spec.dir)?.pid;
    let second = supervised.start("1a2b3c4d", &incarnation, &spec).await?;
    ensure!(state_of(&first).await? == WorkloadState::Idle, "the first handle isn't idle");
    ensure!(state_of(&second).await? == WorkloadState::Idle, "the second handle isn't idle");
    ensure!(Intent::read(&spec.dir)?.pid == pid, "the second start spawned another helper");
    ensure!(supervised.records().len() == 1, "{} records", supervised.records().len());
    ensure!(lock_state(&spec.dir) == LockState::Held, "the helper doesn't hold its lock");
    supervised.stop("1a2b3c4d", Duration::from_secs(1)).await
}

/// A helper signed without a Developer ID certificate says in its log that it checks control peers by user ID only.
///
/// # Errors
///
/// Fails when the log lacks the line.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_development_helper_says_so_in_its_log() -> Result<()> {
    let root = tempfile::tempdir()?;
    let incarnation = Incarnation::random();
    let spec = workload(root.path(), "1a2b3c4d", incarnation)?;
    let supervised = ChildSupervisor::new(root.path().to_path_buf());
    supervised.start("1a2b3c4d", &incarnation, &spec).await?;
    let log = fs::read_to_string(&spec.log)?;
    ensure!(
        log.lines().any(|line| line == "avirtvmd: development build: control peers are checked by user ID only"),
        "the log lacks the development line"
    );
    supervised.stop("1a2b3c4d", Duration::from_secs(1)).await
}

/// Starting with another incarnation fails and leaves the helper running.
///
/// # Errors
///
/// Fails when the second start succeeds or the first helper stopped.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn starting_with_another_incarnation_fails_and_leaves_the_helper_running() -> Result<()> {
    let root = tempfile::tempdir()?;
    let incarnation = Incarnation::random();
    let spec = workload(root.path(), "1a2b3c4d", incarnation)?;
    let supervised = ChildSupervisor::new(root.path().to_path_buf());
    let handle = supervised.start("1a2b3c4d", &incarnation, &spec).await?;
    let Err(err) = supervised.start("1a2b3c4d", &Incarnation::random(), &spec).await else {
        bail!("the start succeeded")
    };
    ensure!(err.to_string().contains("another incarnation"), "the error is {err:#}");
    ensure!(state_of(&handle).await? == WorkloadState::Idle, "the first helper isn't idle");
    supervised.stop("1a2b3c4d", Duration::from_secs(1)).await
}

/// A stop ends the helper, which frees its lock and removes its socket.
///
/// # Errors
///
/// Fails when the helper keeps its lock or socket after the stop.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn stop_is_confirmed_through_the_lock_and_removes_the_socket() -> Result<()> {
    let root = tempfile::tempdir()?;
    let incarnation = Incarnation::random();
    let spec = workload(root.path(), "1a2b3c4d", incarnation)?;
    let supervised = ChildSupervisor::new(root.path().to_path_buf());
    let handle = supervised.start("1a2b3c4d", &incarnation, &spec).await?;
    let before = supervised.list().await?;
    ensure!(before.iter().all(|status| status.state == HelperState::Running), "{before:?}");

    supervised.stop("1a2b3c4d", Duration::from_secs(1)).await?;
    ensure!(lock_state(&spec.dir) == LockState::Free, "the lock isn't free");
    ensure!(!spec.dir.join("v.sock").exists(), "the socket is still there");
    ensure!(handle.event().await.is_none(), "an event arrived after the stop");
    let after = supervised.list().await?;
    ensure!(after.iter().all(|status| status.state == HelperState::Ended), "{after:?}");
    supervised.stop("1a2b3c4d", Duration::from_secs(1)).await
}

/// Closing the helper's stdin makes it exit and frees the lock.
///
/// # Errors
///
/// Fails when the helper exits with an error or keeps its lock.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn closing_the_helper_stdin_makes_it_exit_and_frees_the_lock() -> Result<()> {
    let root = tempfile::tempdir()?;
    let incarnation = Incarnation::random();
    let spec = workload(root.path(), "1a2b3c4d", incarnation)?;
    let supervised = ChildSupervisor::new(root.path().to_path_buf());
    supervised.start("1a2b3c4d", &incarnation, &spec).await?;

    // Take the record apart so that closing stdin is the only thing that happens to the helper.
    let removed = supervised.records().remove("1a2b3c4d").context("the record is missing")?;
    let Record { _stdin: stdin, mut child, .. } = removed;
    drop(stdin);
    let status = time::timeout(Duration::from_secs(10), child.wait()).await??;
    ensure!(status.success(), "the helper exited with {status}");
    ensure!(lock_state(&spec.dir) == LockState::Free, "the lock isn't free");
    Ok(())
}

/// A second supervisor finds a helper it didn't start. It stops the helper and confirms the exit.
///
/// # Errors
///
/// Fails when the second supervisor lists the helper wrongly or fails to stop it.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn another_supervisor_finds_stops_and_confirms_a_helper_it_did_not_start() -> Result<()> {
    let root = tempfile::tempdir()?;
    let incarnation = Incarnation::random();
    let spec = workload(root.path(), "1a2b3c4d", incarnation)?;
    let first = ChildSupervisor::new(root.path().to_path_buf());
    first.start("1a2b3c4d", &incarnation, &spec).await?;
    // A second supervisor has no record, so it finds the helper by its lock and its intent.
    let second = ChildSupervisor::new(root.path().to_path_buf());
    let found = second.list().await?;
    ensure!(found.iter().all(|status| status.state == HelperState::Running), "{found:?}");
    ensure!(found.iter().all(|status| status.incarnation == Some(incarnation)), "the incarnation differs");

    second.stop("1a2b3c4d", Duration::from_secs(1)).await?;
    ensure!(lock_state(&spec.dir) == LockState::Free, "the lock isn't free");
    let gone = second.list().await?;
    ensure!(gone.iter().all(|status| status.state == HelperState::Ended), "{gone:?}");
    first.stop("1a2b3c4d", Duration::from_secs(1)).await
}
