//! Unit tests for finding and starting helpers, against a fake helper.

use super::super::intent::{Intent, State};
use super::super::testing::{FakeHelper, hello};
use super::{
    ChildSupervisor, HelperHandle, HelperSpec, HelperState, HelperStatus, HelperSupervisor as _, Incarnation,
    LockState, lock_state,
};
use crate::runtime::containerization::control::{self, Expected};
use crate::runtime::containerization::proto::{RefusalCode, RequestOp, ResponseResult, Status};
use anyhow::{Context as _, Result, bail, ensure};
use core::time::Duration;
use rustix::process::{self, Pid};
use std::fs::{self, File, OpenOptions};
use std::os::unix::fs as unix_fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tokio::process::{Child as TokioChild, ChildStdin};
use tokio::time::sleep;

/// How long a test waits on a helper that doesn't speak.
const SHORT: Duration = Duration::from_millis(300);

/// The incarnation the launch intents record.
///
/// # Errors
///
/// Fails when the test incarnation doesn't parse.
fn incarnation() -> Result<Incarnation> {
    Incarnation::from_hex("000102030405060708090a0b0c0d0e0f").context("the test incarnation")
}

/// A supervisor with a short probe timeout, for the helpers in `root`.
fn supervisor(root: &Path) -> ChildSupervisor {
    let mut supervised = ChildSupervisor::new(root.to_path_buf());
    supervised.probe_timeout = SHORT;
    supervised
}

/// Lists the helpers in `root`.
///
/// # Errors
///
/// Fails when the listing fails.
async fn listed(root: &Path) -> Result<Vec<HelperStatus>> {
    supervisor(root).list().await
}

/// A workload directory with a launch intent, as the runtime leaves it before a spawn.
///
/// # Errors
///
/// Fails when writing the directory or the intent fails.
fn workload_dir(root: &Path, workload: &str, state: State) -> Result<PathBuf> {
    let dir = root.join(workload);
    fs::create_dir_all(&dir)?;
    let intent = Intent {
        incarnation: incarnation()?.to_hex(),
        pid: None,
        spec_digest: "ab".repeat(32),
        state,
        workload: workload.to_owned(),
    };
    intent.write(&dir)?;
    Ok(dir)
}

/// Takes the helper's lock, as a live helper does, until the caller drops the returned file.
///
/// # Errors
///
/// Fails when creating or locking the lock file fails.
fn hold_lock(dir: &Path) -> Result<File> {
    let file = OpenOptions::new().create(true).truncate(false).write(true).open(dir.join("vmd.lock"))?;
    file.try_lock()?;
    Ok(file)
}

/// The only status in `statuses`.
///
/// # Errors
///
/// Fails when there isn't exactly one.
fn only(statuses: Vec<HelperStatus>) -> Result<HelperStatus> {
    let mut all = statuses.into_iter();
    match (all.next(), all.next()) {
        (Some(status), None) => Ok(status),
        (Some(_) | None, Some(_) | None) => bail!("expected exactly one status"),
    }
}

/// An incarnation round-trips through hex, and anything but 32 lowercase hex digits fails to parse.
///
/// # Errors
///
/// Fails when a parse or a conversion breaks an expectation.
#[test]
fn an_incarnation_round_trips_through_hex_and_refuses_anything_else() -> Result<()> {
    let random = Incarnation::random();
    ensure!(Incarnation::from_hex(&random.to_hex()) == Some(random), "a random incarnation changed");
    ensure!(incarnation()?.to_hex() == "000102030405060708090a0b0c0d0e0f", "the hex form differs");
    ensure!(incarnation()?.as_bytes().first() == Some(&0u8), "the first byte differs");
    let short = "0".repeat(31);
    let long = "0".repeat(33);
    let upper = "A".repeat(32);
    let other = "g".repeat(32);
    for bad in ["", "00", &short, &long, &upper, &other] {
        ensure!(Incarnation::from_hex(bad).is_none(), "{bad:?} parsed");
    }
    ensure!(Incarnation::random() != Incarnation::random(), "two random incarnations matched");
    Ok(())
}

/// A live helper with the recorded incarnation is running.
///
/// # Errors
///
/// Fails when the listing differs from the expected one.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_live_helper_with_the_recorded_incarnation_is_running() -> Result<()> {
    let root = tempfile::tempdir()?;
    let dir = workload_dir(root.path(), "1a2b3c4d", State::Starting)?;
    let _lock = hold_lock(&dir)?;
    let _fake = FakeHelper::spawn(&dir.join("v.sock"), Some(hello("1a2b3c4d", &incarnation()?, 1, 1)))?;
    let status = only(listed(root.path()).await?)?;
    ensure!(status.workload == "1a2b3c4d", "the workload is {}", status.workload);
    ensure!(status.incarnation == Some(incarnation()?), "the incarnation differs");
    ensure!(status.state == HelperState::Running, "the state is {:?}", status.state);
    Ok(())
}

/// A live helper with another workload id or incarnation is unknown.
///
/// # Errors
///
/// Fails when a helper's state differs from the expected one.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_live_helper_with_another_workload_id_or_incarnation_is_unknown() -> Result<()> {
    let root = tempfile::tempdir()?;
    let wrong_workload = workload_dir(root.path(), "1a2b3c4d", State::Starting)?;
    let _lock_a = hold_lock(&wrong_workload)?;
    let _fake_a = FakeHelper::spawn(&wrong_workload.join("v.sock"), Some(hello("deadbeef", &incarnation()?, 1, 1)))?;
    let stale = workload_dir(root.path(), "5e6f7a8b", State::Starting)?;
    let _lock_b = hold_lock(&stale)?;
    let _fake_b = FakeHelper::spawn(&stale.join("v.sock"), Some(hello("5e6f7a8b", &Incarnation::random(), 1, 1)))?;
    let statuses = listed(root.path()).await?;
    ensure!(statuses.len() == 2, "{} helpers listed", statuses.len());
    for status in &statuses {
        ensure!(matches!(status.state, HelperState::Unknown(_)), "{status:?}");
    }
    Ok(())
}

/// A live helper that speaks only other versions is incompatible.
///
/// # Errors
///
/// Fails when the helper's state differs.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_live_helper_with_an_unsupported_version_is_incompatible() -> Result<()> {
    let root = tempfile::tempdir()?;
    let dir = workload_dir(root.path(), "1a2b3c4d", State::Starting)?;
    let _lock = hold_lock(&dir)?;
    let _fake = FakeHelper::spawn(&dir.join("v.sock"), Some(hello("1a2b3c4d", &incarnation()?, 2, 2)))?;
    let status = only(listed(root.path()).await?)?;
    ensure!(matches!(status.state, HelperState::Incompatible(_)), "{status:?}");
    Ok(())
}

/// A live helper that doesn't answer is unknown, not ended.
///
/// # Errors
///
/// Fails when a helper's state differs from the expected one.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_live_helper_that_does_not_answer_is_unknown_and_not_ended() -> Result<()> {
    let root = tempfile::tempdir()?;
    let silent = workload_dir(root.path(), "1a2b3c4d", State::Starting)?;
    let _lock_a = hold_lock(&silent)?;
    let _fake = FakeHelper::spawn(&silent.join("v.sock"), None)?;
    // No socket yet: the helper started but hasn't bound it.
    let unbound = workload_dir(root.path(), "5e6f7a8b", State::Starting)?;
    let _lock_b = hold_lock(&unbound)?;
    let statuses = listed(root.path()).await?;
    ensure!(statuses.len() == 2, "{} helpers listed", statuses.len());
    for status in &statuses {
        ensure!(matches!(status.state, HelperState::Unknown(_)), "{status:?}");
    }
    Ok(())
}

/// A live helper that refuses avirthostd is unknown, and the state says why.
///
/// # Errors
///
/// Fails when the helper's state differs from the expected one.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_live_helper_that_refuses_avirthostd_is_unknown() -> Result<()> {
    let root = tempfile::tempdir()?;
    let dir = workload_dir(root.path(), "1a2b3c4d", State::Starting)?;
    let _lock = hold_lock(&dir)?;
    let _fake =
        FakeHelper::spawn_refusing_peers(&dir.join("v.sock"), RefusalCode::WrongPeer, "the peer isn't avirthostd")?;
    let status = only(listed(root.path()).await?)?;
    let HelperState::Unknown(why) = &status.state else { bail!("the state is {:?}", status.state) };
    ensure!(why.contains("refused the connection") && why.contains("the peer isn't avirthostd"), "{why}");
    Ok(())
}

/// A helper whose lock is free or missing has ended.
///
/// # Errors
///
/// Fails when a helper's state differs from the expected one.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_helper_whose_lock_is_free_or_missing_has_ended() -> Result<()> {
    let root = tempfile::tempdir()?;
    let never_started = workload_dir(root.path(), "1a2b3c4d", State::Intended)?;
    let exited = workload_dir(root.path(), "5e6f7a8b", State::Running)?;
    drop(hold_lock(&exited)?);
    ensure!(lock_state(&never_started) == LockState::Missing, "the missing lock isn't missing");
    ensure!(lock_state(&exited) == LockState::Free, "the released lock isn't free");
    let statuses = listed(root.path()).await?;
    let states: Vec<_> = statuses.iter().map(|status| &status.state).collect();
    ensure!(states == [&HelperState::Ended, &HelperState::Ended], "the states are {states:?}");
    let recorded: Vec<_> = statuses.iter().map(|status| status.recorded).collect();
    ensure!(recorded.contains(&Some(State::Intended)) && recorded.contains(&Some(State::Running)), "{recorded:?}");
    Ok(())
}

/// A directory without an intent isn't listed, and a damaged intent makes the helper unknown.
///
/// # Errors
///
/// Fails when the listing differs from the expected one.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_directory_without_an_intent_is_not_listed_and_a_damaged_intent_is_unknown() -> Result<()> {
    let root = tempfile::tempdir()?;
    fs::create_dir_all(root.path().join("00000000"))?;
    let damaged = root.path().join("1a2b3c4d");
    fs::create_dir_all(&damaged)?;
    fs::write(damaged.join("intent.json"), b"{")?;
    let status = only(listed(root.path()).await?)?;
    ensure!(status.workload == "1a2b3c4d", "the workload is {}", status.workload);
    ensure!(matches!(status.state, HelperState::Unknown(_)), "{status:?}");
    ensure!(status.recorded.is_none(), "{status:?}");
    Ok(())
}

/// An ended intent with a held lock is unknown, and a listing doesn't probe it.
///
/// # Errors
///
/// Fails when the status differs from the expected one or the listing waits for a probe.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn an_ended_intent_with_a_held_lock_is_unknown_and_not_probed() -> Result<()> {
    let root = tempfile::tempdir()?;
    let dir = workload_dir(root.path(), "1a2b3c4d", State::Ended)?;
    let _lock = hold_lock(&dir)?;
    let started = Instant::now();
    let status = only(listed(root.path()).await?)?;
    ensure!(started.elapsed() < SHORT, "the listing waited for a probe");
    let HelperState::Unknown(why) = &status.state else { bail!("the state is {:?}", status.state) };
    ensure!(why.contains("records that the helper ended, but its lock is held"), "the text is {why}");
    Ok(())
}

/// An ended intent has ended when opening its lock file fails.
///
/// # Errors
///
/// Fails when the status isn't `Ended`.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn an_ended_intent_with_an_unreadable_lock_has_ended() -> Result<()> {
    let root = tempfile::tempdir()?;
    let dir = workload_dir(root.path(), "1a2b3c4d", State::Ended)?;
    let lock = dir.join("vmd.lock");
    fs::write(&lock, b"")?;
    fs::set_permissions(&lock, fs::Permissions::from_mode(0o000))?;
    // Root opens the file anyway, so the check can't fail for it.
    if File::open(&lock).is_ok() {
        return Ok(());
    }
    let status = only(listed(root.path()).await?)?;
    ensure!(status.state == HelperState::Ended, "the state is {:?}", status.state);
    Ok(())
}

/// A listing reports the state each intent records.
///
/// # Errors
///
/// Fails when a recorded state differs from the intent.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_listing_reports_the_recorded_state() -> Result<()> {
    let root = tempfile::tempdir()?;
    let states = [State::Ended, State::Intended, State::Running, State::Starting, State::Stopping];
    let ids = ["00000001", "00000002", "00000003", "00000004", "00000005"];
    for (id, state) in ids.iter().zip(states) {
        workload_dir(root.path(), id, state)?;
    }
    let statuses = listed(root.path()).await?;
    ensure!(statuses.len() == ids.len(), "the listing has {} statuses", statuses.len());
    for status in statuses {
        let index = ids.iter().position(|id| *id == status.workload).context("an unexpected workload")?;
        ensure!(status.recorded == states.get(index).copied(), "{status:?}");
    }
    Ok(())
}

/// Files and links in the workloads directory aren't helpers.
///
/// # Errors
///
/// Fails when the listing isn't empty.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn files_and_links_in_the_workloads_directory_are_not_helpers() -> Result<()> {
    let root = tempfile::tempdir()?;
    fs::write(root.path().join("stray"), b"x")?;
    unix_fs::symlink(root.path(), root.path().join("link"))?;
    ensure!(listed(root.path()).await?.is_empty(), "a stray entry was listed");
    Ok(())
}

/// A listing that can't read the directory is an error and not an empty list.
///
/// # Errors
///
/// Fails when the listing of a missing directory succeeds.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_listing_that_cannot_read_the_directory_is_an_error_and_not_an_empty_list() -> Result<()> {
    let root = tempfile::tempdir()?;
    ensure!(listed(&root.path().join("workloads")).await.is_err(), "the listing of a missing directory succeeded");
    Ok(())
}

/// Stopping a helper that already ended, or never existed, succeeds.
///
/// # Errors
///
/// Fails when either stop fails.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn stopping_a_helper_that_is_already_gone_succeeds() -> Result<()> {
    let root = tempfile::tempdir()?;
    let exited = workload_dir(root.path(), "5e6f7a8b", State::Running)?;
    drop(hold_lock(&exited)?);
    let supervised = supervisor(root.path());
    supervised.stop("5e6f7a8b", Duration::ZERO).await?;
    ensure!(Intent::read(&exited)?.state == State::Ended, "the intent doesn't record the end");
    supervised.stop("00000000", Duration::ZERO).await
}

/// Stopping a helper whose lock is free records that it ended and clears its pid.
///
/// # Errors
///
/// Fails when the stop fails or the intent differs from the expected one.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn stopping_a_helper_that_is_already_gone_records_that_it_ended() -> Result<()> {
    let root = tempfile::tempdir()?;
    let exited = workload_dir(root.path(), "5e6f7a8b", State::Running)?;
    let mut intent = Intent::read(&exited)?;
    intent.pid = Some(4242);
    intent.write(&exited)?;
    drop(hold_lock(&exited)?);
    supervisor(root.path()).stop("5e6f7a8b", Duration::ZERO).await?;
    let after = Intent::read(&exited)?;
    ensure!(after.state == State::Ended && after.pid.is_none(), "the intent is {after:?}");
    Ok(())
}

/// A second stop of an ended helper leaves the intent alone.
///
/// # Errors
///
/// Fails when the stop fails or rewrites the intent.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_second_stop_of_an_ended_helper_does_nothing() -> Result<()> {
    let root = tempfile::tempdir()?;
    let dir = workload_dir(root.path(), "5e6f7a8b", State::Ended)?;
    let before = fs::read(dir.join("intent.json"))?;
    supervisor(root.path()).stop("5e6f7a8b", Duration::ZERO).await?;
    ensure!(fs::read(dir.join("intent.json"))? == before, "the intent changed");
    Ok(())
}

/// Makes `dir` read-only, so a read of the intent works and a rewrite fails. Returns whether the
/// directory is unwritable, which root can always write.
///
/// # Errors
///
/// Fails when changing the permissions fails.
fn make_unwritable(dir: &Path) -> Result<bool> {
    fs::set_permissions(dir, fs::Permissions::from_mode(0o500))?;
    Ok(File::create(dir.join("probe")).is_err())
}

/// Makes `dir` writable again, so the temporary directory can clean up.
///
/// # Errors
///
/// Fails when changing the permissions fails.
fn make_writable(dir: &Path) -> Result<()> {
    Ok(fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?)
}

/// A stop of a helper that keeps its lock leaves the intent at `stopping` with its pid.
///
/// # Errors
///
/// Fails when the stop succeeds or the intent differs from the expected one.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_stop_that_cannot_confirm_the_exit_leaves_the_intent_stopping() -> Result<()> {
    let root = tempfile::tempdir()?;
    let dir = workload_dir(root.path(), "5e6f7a8b", State::Running)?;
    let _lock = hold_lock(&dir)?;
    let Err(err) = supervisor(root.path()).stop("5e6f7a8b", Duration::ZERO).await else {
        bail!("the stop succeeded");
    };
    ensure!(err.to_string().contains("still running"), "the error is {err:#}");
    let after = Intent::read(&dir)?;
    ensure!(after.state == State::Stopping && after.pid.is_none(), "the intent is {after:?}");
    Ok(())
}

/// Stops workload `5e6f7a8b` after making its directory read-only, and returns the error text.
///
/// Returns `None` for root, which writes to the directory anyway.
///
/// # Errors
///
/// Fails when preparing the directory fails or the stop succeeds.
async fn stop_without_write_access(state: State, lock_held: bool) -> Result<Option<String>> {
    let root = tempfile::tempdir()?;
    let dir = workload_dir(root.path(), "5e6f7a8b", state)?;
    let lock = hold_lock(&dir)?;
    if !lock_held {
        drop(lock);
    }
    if !make_unwritable(&dir)? {
        return Ok(None);
    }
    let outcome = supervisor(root.path()).stop("5e6f7a8b", Duration::ZERO).await;
    make_writable(&dir)?;
    let Err(err) = outcome else { bail!("the stop succeeded") };
    Ok(Some(format!("{err:#}")))
}

/// A failed write of the end fails the stop, and the error includes the workload.
///
/// # Errors
///
/// Fails when the stop succeeds or its error lacks the workload.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_failed_write_of_the_end_fails_the_stop() -> Result<()> {
    let Some(text) = stop_without_write_access(State::Running, false).await? else { return Ok(()) };
    ensure!(text.contains("record that the helper of workload 5e6f7a8b ended"), "the error is {text}");
    Ok(())
}

/// A failed write of `stopping`, then a failed stop, gives an error that includes both failures.
///
/// # Errors
///
/// Fails when the stop succeeds or its error lacks either failure.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_failed_write_of_stopping_is_in_the_error_when_the_stop_fails() -> Result<()> {
    let Some(text) = stop_without_write_access(State::Running, true).await? else { return Ok(()) };
    ensure!(text.contains("still running") && text.contains("was not rewritten to stopping"), "the error is {text}");
    Ok(())
}

/// A stop leaves an intent that already says `stopping` alone, so no write fails.
///
/// # Errors
///
/// Fails when the stop succeeds or its error includes a write.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_stop_does_not_rewrite_an_intent_that_says_stopping() -> Result<()> {
    let Some(text) = stop_without_write_access(State::Stopping, true).await? else { return Ok(()) };
    ensure!(text.contains("still running") && !text.contains("rewritten"), "the error is {text}");
    Ok(())
}

/// A helper that exits before it binds its socket fails the start.
///
/// # Errors
///
/// Fails when the start succeeds or the intent differs from the expected one.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_started_helper_that_exits_before_binding_fails_the_start() -> Result<()> {
    let root = tempfile::tempdir()?;
    let dir = workload_dir(root.path(), "1a2b3c4d", State::Intended)?;
    let spec =
        HelperSpec { bin: PathBuf::from("/usr/bin/false"), dir: dir.clone(), log: root.path().join("helper.log") };
    let Err(err) = supervisor(root.path()).start("1a2b3c4d", &incarnation()?, &spec).await else {
        bail!("the start succeeded");
    };
    ensure!(err.to_string().contains("exited during startup"), "the error is {err:#}");
    let intent = Intent::read(&dir)?;
    ensure!(intent.state == State::Starting && intent.pid.is_some(), "the intent is {intent:?}");
    Ok(())
}

/// A helper that has exited before it receives the incarnation fails the start as an exit during startup.
///
/// The test waits for the helper to exit first. The write then fails every time.
///
/// # Errors
///
/// Fails when the start succeeds, the error isn't the exit report, or the intent isn't `starting`.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_helper_that_exited_before_the_incarnation_is_written_fails_the_start() -> Result<()> {
    let root = tempfile::tempdir()?;
    let dir = workload_dir(root.path(), "1a2b3c4d", State::Intended)?;
    let spec =
        HelperSpec { bin: PathBuf::from("/usr/bin/false"), dir: dir.clone(), log: root.path().join("helper.log") };
    let (mut child, mut stdin) = ChildSupervisor::launch("1a2b3c4d", &spec)?;
    child.wait().await?;
    let expected = Expected { incarnation: &incarnation()?, workload: "1a2b3c4d" };
    let Err(err) = ChildSupervisor::handshake(&mut child, &mut stdin, &expected, &spec).await else {
        bail!("the handshake succeeded");
    };
    ensure!(err.to_string().contains("exited during startup"), "the error is {err:#}");
    let intent = Intent::read(&dir)?;
    ensure!(intent.state == State::Starting, "the intent is {intent:?}");
    Ok(())
}

/// The error chain of a handshake that fails.
///
/// # Errors
///
/// Fails when the handshake succeeds.
async fn handshake_error(child: &mut TokioChild, stdin: &mut ChildStdin, spec: &HelperSpec) -> Result<String> {
    let expected = Expected { incarnation: &incarnation()?, workload: "1a2b3c4d" };
    let Err(err) = ChildSupervisor::handshake(child, stdin, &expected, spec).await else {
        bail!("the handshake succeeded");
    };
    Ok(format!("{err:#}"))
}

/// A failed write of the incarnation stays in the error when recording the spawn fails too.
///
/// The test waits for the helper to exit and makes the workload directory read-only, so the write fails and
/// the handshake can't record the spawn. It returns early for root, which writes to the directory anyway.
///
/// # Errors
///
/// Fails when the handshake succeeds or its error lacks either failure.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_failed_record_of_the_spawn_keeps_the_write_error() -> Result<()> {
    let root = tempfile::tempdir()?;
    let dir = workload_dir(root.path(), "1a2b3c4d", State::Intended)?;
    let spec =
        HelperSpec { bin: PathBuf::from("/usr/bin/false"), dir: dir.clone(), log: root.path().join("helper.log") };
    let (mut child, mut stdin) = ChildSupervisor::launch("1a2b3c4d", &spec)?;
    child.wait().await?;
    if !make_unwritable(&dir)? {
        return Ok(());
    }
    let expected = Expected { incarnation: &incarnation()?, workload: "1a2b3c4d" };
    let outcome = ChildSupervisor::handshake(&mut child, &mut stdin, &expected, &spec).await;
    make_writable(&dir)?;
    let Err(err) = outcome else { bail!("the handshake succeeded") };
    let text = format!("{err:#}");
    ensure!(
        text.contains("write the incarnation to avirtvmd") && text.contains("record the helper's pid"),
        "the error is {text}"
    );
    Ok(())
}

/// A failed write of the incarnation stays in the error when waiting for the helper fails too.
///
/// The test reaps the helper itself, so the write fails and the wait fails with `No child processes`.
///
/// # Errors
///
/// Fails when the handshake succeeds or its error lacks either failure.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_failed_wait_for_the_helper_keeps_the_write_error() -> Result<()> {
    let root = tempfile::tempdir()?;
    let spec = HelperSpec {
        bin: PathBuf::from("/usr/bin/false"),
        dir: workload_dir(root.path(), "1a2b3c4d", State::Intended)?,
        log: root.path().join("helper.log"),
    };
    let (mut child, mut stdin) = ChildSupervisor::launch("1a2b3c4d", &spec)?;
    let pid = child.id().and_then(|raw| i32::try_from(raw).ok()).and_then(Pid::from_raw).context("the helper's pid")?;
    process::waitpid(Some(pid), process::WaitOptions::empty())?;
    let text = handshake_error(&mut child, &mut stdin, &spec).await?;
    ensure!(
        text.contains("write the incarnation to avirtvmd") && text.contains("No child processes"),
        "the error is {text}"
    );
    Ok(())
}

/// A helper that closes its stdin and keeps running fails the start with the write error once `BIND_TIMEOUT` passes.
///
/// The helper creates `closed` after it closes its stdin, and the test waits for that file before the write. The
/// write then fails, and the helper keeps running until the test kills it.
///
/// # Errors
///
/// Fails when the start succeeds, the error isn't the write error, the intent isn't `starting`, or the helper has
/// exited.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_helper_that_closes_stdin_and_keeps_running_fails_the_start_with_the_write_error() -> Result<()> {
    let root = tempfile::tempdir()?;
    let dir = workload_dir(root.path(), "1a2b3c4d", State::Intended)?;
    let bin = root.path().join("helper.sh");
    fs::write(&bin, "#!/bin/sh\nexec 0<&-\n: > \"$5/closed\"\nexec sleep 30\n")?;
    fs::set_permissions(&bin, fs::Permissions::from_mode(0o700))?;
    let spec = HelperSpec { bin, dir: dir.clone(), log: root.path().join("helper.log") };
    let (mut child, mut stdin) = ChildSupervisor::launch("1a2b3c4d", &spec)?;
    let started = Instant::now();
    while !dir.join("closed").exists() {
        ensure!(started.elapsed() < Duration::from_secs(5), "the helper never closed its stdin");
        sleep(Duration::from_millis(10)).await;
    }
    let expected = Expected { incarnation: &incarnation()?, workload: "1a2b3c4d" };
    let Err(err) = ChildSupervisor::handshake(&mut child, &mut stdin, &expected, &spec).await else {
        bail!("the handshake succeeded");
    };
    let text = format!("{err:#}");
    ensure!(
        text.contains("write the incarnation to avirtvmd") && !text.contains("exited during startup"),
        "the error is {text}"
    );
    let intent = Intent::read(&dir)?;
    ensure!(intent.state == State::Starting && intent.pid.is_some(), "the intent is {intent:?}");
    ensure!(child.try_wait()?.is_none(), "the helper exited");
    child.kill().await?;
    Ok(())
}

/// A missing binary fails the start before any intent changes.
///
/// # Errors
///
/// Fails when the start succeeds or the intent changed.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_missing_binary_fails_the_start_before_any_intent_changes() -> Result<()> {
    let root = tempfile::tempdir()?;
    let dir = workload_dir(root.path(), "1a2b3c4d", State::Intended)?;
    let spec =
        HelperSpec { bin: root.path().join("no-such-avirtvmd"), dir: dir.clone(), log: root.path().join("helper.log") };
    ensure!(supervisor(root.path()).start("1a2b3c4d", &incarnation()?, &spec).await.is_err(), "the start succeeded");
    ensure!(Intent::read(&dir)?.state == State::Intended, "the intent changed");
    Ok(())
}

/// A handle over a live connection answers requests and delivers events.
///
/// # Errors
///
/// Fails when the handshake, the request, or the event breaks an expectation.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_handle_over_a_live_connection_answers_requests() -> Result<()> {
    let root = tempfile::tempdir()?;
    let dir = workload_dir(root.path(), "1a2b3c4d", State::Starting)?;
    let _fake = FakeHelper::spawn(&dir.join("v.sock"), Some(hello("1a2b3c4d", &incarnation()?, 1, 1)))?;
    let expected_incarnation = incarnation()?;
    let expected = Expected { incarnation: &expected_incarnation, workload: "1a2b3c4d" };
    let Ok(client) = control::connect(&dir.join("v.sock"), &expected, SHORT).await else {
        bail!("the handshake failed")
    };
    let handle = HelperHandle(client);
    ensure!(matches!(handle.request(RequestOp::Status(Status)).await?, ResponseResult::Status(_)), "not a status");
    ensure!(handle.event().await.is_some(), "no event arrived");
    ensure!(!handle.is_closed(), "the handle is closed");
    Ok(())
}
