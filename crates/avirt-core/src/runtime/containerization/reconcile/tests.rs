//! Unit tests for the reaper.
//!
//! Each test builds a state root on disk, with a workload directory and VM state for every id it
//! names, and a supervisor that reports whatever the test says. No VM starts.

use super::reap;
use crate::runtime::containerization::intent::{Intent, State};
use crate::runtime::containerization::lifecycle::{
    HelperHandle, HelperSpec, HelperState, HelperStatus, HelperSupervisor, Incarnation,
};
use crate::state::Root;
use anyhow::{Result, bail, ensure};
use async_trait::async_trait;
use core::time::Duration;
use std::fs;
use std::sync::{Mutex, PoisonError};

/// A supervisor that reports what a test tells it to.
#[derive(Default)]
struct Canned {
    /// Workloads whose helper has ended.
    ended: Vec<&'static str>,
    /// Whether `list` fails.
    list_fails: bool,
    /// The state that the launch intent of each stubborn workload records.
    recorded: Option<State>,
    /// Workloads with a live helper.
    running: Vec<&'static str>,
    /// The workloads that the reaper asked `stop` about.
    stopped: Mutex<Vec<String>>,
    /// Workloads that are still alive after a stop.
    stubborn: Vec<&'static str>,
}

#[async_trait]
impl HelperSupervisor for Canned {
    async fn list(&self) -> Result<Vec<HelperStatus>> {
        if self.list_fails {
            bail!("the listing failed");
        }
        let status = |workload: &&str, state: HelperState| HelperStatus {
            incarnation: Some(Incarnation::random()),
            recorded: self.recorded,
            state,
            workload: (*workload).to_owned(),
        };
        let mut all: Vec<HelperStatus> = self.ended.iter().map(|id| status(id, HelperState::Ended)).collect();
        all.extend(self.running.iter().map(|id| status(id, HelperState::Running)));
        all.extend(self.stubborn.iter().map(|id| status(id, HelperState::Unknown("silent".to_owned()))));
        Ok(all)
    }

    async fn start(&self, _workload: &str, _incarnation: &Incarnation, _spec: &HelperSpec) -> Result<HelperHandle> {
        bail!("the canned supervisor starts nothing")
    }

    async fn stop(&self, workload: &str, _grace: Duration) -> Result<()> {
        self.stopped.lock().unwrap_or_else(PoisonError::into_inner).push(workload.to_owned());
        if self.stubborn.contains(&workload) {
            bail!("still running");
        }
        Ok(())
    }
}

/// A state root with a workload directory (and VM state) for each id.
///
/// # Errors
///
/// Fails when making the directories fails.
fn root_with(ids: &[&str]) -> Result<(Root, tempfile::TempDir)> {
    let base = tempfile::tempdir()?;
    let root = Root::open_in(base.path())?;
    for id in ids {
        let dir = root.workloads().join(id);
        fs::create_dir_all(&dir)?;
        fs::create_dir_all(root.containers().join(id))?;
        let intent = Intent {
            incarnation: Incarnation::random().to_hex(),
            pid: None,
            spec_digest: "ab".repeat(32),
            state: State::Starting,
            workload: (*id).to_owned(),
        };
        intent.write(&dir)?;
    }
    Ok((root, base))
}

/// The reaper clears ended helpers, and it stops live ones before it clears them.
///
/// # Errors
///
/// Fails when a directory remains, an ended helper gets a stop, or a live helper doesn't.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn ended_helpers_are_cleared_and_live_helpers_are_stopped_first() -> Result<()> {
    let (root, _base) = root_with(&["1a2b3c4d", "5e6f7a8b"])?;
    let canned = Canned { ended: vec!["1a2b3c4d"], running: vec!["5e6f7a8b"], ..Canned::default() };
    let swept = reap(&canned, &root).await;
    ensure!(swept.removed == ["1a2b3c4d", "5e6f7a8b"] && swept.kept.is_empty(), "{swept:?}");
    for id in ["1a2b3c4d", "5e6f7a8b"] {
        ensure!(!root.workloads().join(id).exists(), "the workload directory of {id} remains");
        ensure!(!root.containers().join(id).exists(), "the VM state of {id} remains");
    }
    ensure!(canned.stopped.lock().unwrap_or_else(PoisonError::into_inner).as_slice() == ["5e6f7a8b"], "wrong stops");
    Ok(())
}

/// A helper that outlasts its stop keeps its directories and shows up in `kept`.
///
/// # Errors
///
/// Fails when a directory goes or the workload isn't kept.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_helper_that_cannot_be_confirmed_gone_keeps_its_state() -> Result<()> {
    let (root, _base) = root_with(&["1a2b3c4d", "5e6f7a8b"])?;
    let canned = Canned { ended: vec!["5e6f7a8b"], stubborn: vec!["1a2b3c4d"], ..Canned::default() };
    let swept = reap(&canned, &root).await;
    ensure!(swept.kept == ["1a2b3c4d"] && swept.removed == ["5e6f7a8b"], "{swept:?}");
    ensure!(root.workloads().join("1a2b3c4d").is_dir() && root.containers().join("1a2b3c4d").is_dir(), "state went");
    ensure!(!swept.failures.is_empty(), "no failure was reported");
    Ok(())
}

/// A failure line names the state that the launch intent records.
///
/// # Errors
///
/// Fails when the line lacks the state.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_failure_line_names_the_recorded_state() -> Result<()> {
    let (root, _base) = root_with(&["1a2b3c4d"])?;
    let canned = Canned { recorded: Some(State::Stopping), stubborn: vec!["1a2b3c4d"], ..Canned::default() };
    let swept = reap(&canned, &root).await;
    ensure!(swept.failures.iter().any(|line| line.contains("workload 1a2b3c4d (stopping):")), "{swept:?}");
    Ok(())
}

/// A failed listing leaves every directory in place and keeps every workload with an intent.
///
/// # Errors
///
/// Fails when anything goes or a workload with an intent isn't kept.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_failed_listing_removes_nothing() -> Result<()> {
    let (root, _base) = root_with(&["1a2b3c4d", "5e6f7a8b"])?;
    fs::create_dir_all(root.workloads().join("00000000"))?;
    let canned = Canned { list_fails: true, ..Canned::default() };
    let swept = reap(&canned, &root).await;
    ensure!(swept.removed.is_empty() && swept.kept == ["1a2b3c4d", "5e6f7a8b"], "{swept:?}");
    ensure!(
        root.containers().join("1a2b3c4d").is_dir() && root.containers().join("5e6f7a8b").is_dir(),
        "VM state went"
    );
    Ok(())
}

/// The reaper removes VM state with no workload directory beside it.
///
/// # Errors
///
/// Fails when the orphan remains or a paired entry goes.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn an_orphan_container_directory_is_removed() -> Result<()> {
    let (root, _base) = root_with(&["1a2b3c4d"])?;
    fs::create_dir_all(root.containers().join("deadbeef"))?;
    let canned = Canned { running: vec!["1a2b3c4d"], stubborn: vec![], ..Canned::default() };
    drop(reap(&canned, &root).await);
    ensure!(!root.containers().join("deadbeef").exists(), "the orphan remains");
    Ok(())
}
