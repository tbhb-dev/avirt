//! Unit tests for the workload directory, with `leave` and the socket path limit.

use super::{SUN_PATH_MAX, WorkloadDir};
use anyhow::{Result, ensure};
use std::path::PathBuf;

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

/// The longest socket name decides whether the directory fits.
///
/// A directory whose `proxy.sock` path sits one byte under the limit once passed the check, though
/// `mediator.sock` is longer and overflows `sockaddr_un`.
///
/// # Errors
///
/// Fails when `create` accepts the directory or the error doesn't name `mediator.sock`.
#[test]
fn the_longest_socket_name_decides_whether_the_directory_fits() -> Result<()> {
    let id = "00000001";
    let fixed = "/".len() + id.len() + "/proxy.sock".len();
    let filler = SUN_PATH_MAX.saturating_sub(fixed).saturating_sub(2);
    let parent = PathBuf::from(format!("/{}", "a".repeat(filler)));
    ensure!(
        parent.join(id).join("proxy.sock").as_os_str().len() == SUN_PATH_MAX - 1,
        "the fixture path isn't one byte under the limit"
    );
    let err = WorkloadDir::create(&parent, id).err().map(|err| err.to_string());
    ensure!(
        err.is_some_and(|text| text.contains("mediator.sock") && text.contains("too long")),
        "the directory was accepted"
    );
    Ok(())
}
