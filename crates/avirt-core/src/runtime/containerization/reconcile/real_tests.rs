//! Reaper tests that start a real avirtvmd, behind the `avirtvmd-tests` feature.
//!
//! `mise run check-vmd` builds the helper with `mise run build-vmd` and sets `AVIRT_VMD_BIN` to it before it runs these tests.

#![cfg(feature = "avirtvmd-tests")]

use super::reap;
use crate::runtime::containerization::intent::{Intent, State};
use crate::runtime::containerization::lifecycle::{ChildSupervisor, HelperSpec, HelperSupervisor as _, Incarnation};
use crate::state::Root;
use anyhow::{Context as _, Result, ensure};
use core::time::Duration;
use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::PathBuf;

/// A workload under `root` with its launch intent, and the spec to start a helper in it.
///
/// # Errors
///
/// Fails when writing the directories or the intent fails.
fn workload(root: &Root, id: &str, incarnation: Incarnation) -> Result<HelperSpec> {
    let dir = root.workloads().join(id);
    fs::create_dir_all(&dir)?;
    fs::create_dir_all(root.containers().join(id))?;
    let intent = Intent {
        incarnation: incarnation.to_hex(),
        pid: None,
        spec_digest: "ab".repeat(32),
        state: State::Intended,
        workload: id.to_owned(),
    };
    intent.write(&dir)?;
    let bin = env::var_os("AVIRT_VMD_BIN").map(PathBuf::from).context("set AVIRT_VMD_BIN to the built avirtvmd")?;
    fs::create_dir_all(root.scratch())?;
    Ok(HelperSpec { bin, dir, log: root.scratch().join(format!("{id}.avirtvmd.log")) })
}

/// The reaper clears a launch that stopped after the intent, with no helper, and doesn't send a stop.
///
/// # Errors
///
/// Fails when the workload's directories remain.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_crash_after_the_intent_is_reaped() -> Result<()> {
    let base = tempfile::tempdir()?;
    let root = Root::open_in(base.path())?;
    drop(workload(&root, "1a2b3c4d", Incarnation::random())?);
    let swept = reap(&ChildSupervisor::new(root.workloads()), &root).await;
    ensure!(swept.removed == ["1a2b3c4d"], "{swept:?}");
    ensure!(
        !root.workloads().join("1a2b3c4d").exists() && !root.containers().join("1a2b3c4d").exists(),
        "state remains"
    );
    Ok(())
}

/// The reaper stops a helper that a dead avirthostd left running, through its incarnation. A helper
/// under another state root keeps running.
///
/// # Errors
///
/// Fails when the first helper outlasts the reaper or the second helper loses anything.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_crash_after_the_spawn_is_reaped_and_another_root_is_untouched() -> Result<()> {
    let (base, other_base) = (tempfile::tempdir()?, tempfile::tempdir()?);
    let (root, other) = (Root::open_in(base.path())?, Root::open_in(other_base.path())?);
    let (incarnation, other_incarnation) = (Incarnation::random(), Incarnation::random());
    let (spec, other_spec) =
        (workload(&root, "1a2b3c4d", incarnation)?, workload(&other, "5e6f7a8b", other_incarnation)?);
    // Both supervisors live on, so both helpers keep their stdin open, as under a live avirthostd.
    let (first, second) = (ChildSupervisor::new(root.workloads()), ChildSupervisor::new(other.workloads()));
    first.start("1a2b3c4d", &incarnation, &spec).await?;
    second.start("5e6f7a8b", &other_incarnation, &other_spec).await?;

    // A fresh supervisor, as a restarted avirthostd has, finds the first helper by its lock.
    let swept = reap(&ChildSupervisor::new(root.workloads()), &root).await;
    ensure!(swept.removed == ["1a2b3c4d"] && swept.kept.is_empty(), "{swept:?}");
    ensure!(!root.workloads().join("1a2b3c4d").exists(), "the first workload directory remains");
    ensure!(other.workloads().join("5e6f7a8b").is_dir(), "the other root lost a directory");
    let alive = ChildSupervisor::new(other.workloads()).list().await?;
    ensure!(alive.iter().all(|status| status.workload == "5e6f7a8b"), "the other root lists another workload");
    ensure!(second.stop("5e6f7a8b", Duration::from_secs(1)).await.is_ok(), "the other helper didn't stop cleanly");
    drop(first);
    Ok(())
}

/// Reaps `root` with a fresh supervisor, as a restarted avirthostd does, then checks that workload
/// `1a2b3c4d` and its VM state no longer exist.
///
/// # Errors
///
/// Fails when the workload remains in `kept` or either directory remains.
async fn reap_and_ensure_removed(root: &Root) -> Result<()> {
    let swept = reap(&ChildSupervisor::new(root.workloads()), root).await;
    ensure!(swept.removed == ["1a2b3c4d"] && swept.kept.is_empty(), "{swept:?}");
    ensure!(!root.workloads().join("1a2b3c4d").exists(), "the workload directory remains");
    ensure!(!root.containers().join("1a2b3c4d").exists(), "the VM state remains");
    Ok(())
}

/// The reaper stops a helper after avirthostd died during the stop.
///
/// # Errors
///
/// Fails when the helper outlasts the reaper or its directories remain.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_crash_while_stopping_is_reaped() -> Result<()> {
    let base = tempfile::tempdir()?;
    let root = Root::open_in(base.path())?;
    let incarnation = Incarnation::random();
    let spec = workload(&root, "1a2b3c4d", incarnation)?;
    // Dropping the first supervisor would close the helper's stdin, so it outlasts the reap.
    let first = ChildSupervisor::new(root.workloads());
    first.start("1a2b3c4d", &incarnation, &spec).await?;
    let mut intent = Intent::read(&spec.dir)?;
    intent.state = State::Stopping;
    intent.write(&spec.dir)?;

    reap_and_ensure_removed(&root).await?;
    drop(first);
    Ok(())
}

/// The reaper removes a workload whose intent records the end, without a stop, when the lock check
/// fails.
///
/// # Errors
///
/// Fails when the stop doesn't record the end or the directories remain.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_crash_after_the_end_is_recorded_is_reaped_without_a_dial() -> Result<()> {
    let base = tempfile::tempdir()?;
    let root = Root::open_in(base.path())?;
    let incarnation = Incarnation::random();
    let spec = workload(&root, "1a2b3c4d", incarnation)?;
    let supervisor = ChildSupervisor::new(root.workloads());
    supervisor.start("1a2b3c4d", &incarnation, &spec).await?;
    supervisor.stop("1a2b3c4d", Duration::from_secs(5)).await?;
    ensure!(Intent::read(&spec.dir)?.state == State::Ended, "the stop didn't record the end");
    // The helper has exited, so nothing answers on its socket.
    drop(fs::remove_file(spec.dir.join("v.sock")));
    // With a lock file that won't open, the record is the only evidence that the helper ended.
    // Without the record, the reaper sends a stop and keeps a workload it can't confirm gone.
    // Root opens the file anyway, so for root the lock check doesn't fail.
    fs::set_permissions(spec.dir.join("vmd.lock"), fs::Permissions::from_mode(0o000))?;

    reap_and_ensure_removed(&root).await?;
    Ok(())
}
