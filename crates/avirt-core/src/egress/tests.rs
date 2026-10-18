//! Unit tests for the workload directory.

use super::WorkloadDir;
use anyhow::{Result, ensure};

/// Dropping the directory removes it, and leaving it keeps it for the next sweep.
///
/// # Errors
///
/// Fails when creating the directories or checking them breaks an expectation.
#[test]
fn dropping_removes_the_directory_and_leaving_keeps_it() -> Result<()> {
    let base = tempfile::tempdir()?;
    let dropped = WorkloadDir::create(base.path(), "00000001")?;
    let dropped_path = dropped.path().to_path_buf();
    drop(dropped);
    ensure!(!dropped_path.exists(), "dropping kept {}", dropped_path.display());
    let left = WorkloadDir::create(base.path(), "00000002")?;
    let left_path = left.path().to_path_buf();
    left.leave();
    ensure!(left_path.is_dir(), "leaving removed {}", left_path.display());
    Ok(())
}
