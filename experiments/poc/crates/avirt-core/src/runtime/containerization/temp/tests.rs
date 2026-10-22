//! Unit tests for the self-removing temporary paths.
//!
//! They work in a fresh temporary directory and never reach the real caches. The guard and the startup sweep each get their own tests.

use super::{PREFIX, Temp, sweep_stale};
use anyhow::{Result, ensure};
use std::fs;

/// Dropping the guard removes a directory tree and a file.
///
/// # Errors
///
/// Fails when either path remains.
#[test]
fn dropping_the_guard_removes_a_tree_and_a_file() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let tree = dir.path().join(".tmp-tree");
    fs::create_dir_all(tree.join("inner"))?;
    fs::write(tree.join("inner/data"), b"x")?;
    let file = dir.path().join(".tmp-file");
    fs::write(&file, b"x")?;
    drop(Temp::new(tree.clone()));
    drop(Temp::new(file.clone()));
    ensure!(!tree.exists() && !file.exists(), "a temporary path remains");
    Ok(())
}

/// A guard that moved its path into place leaves it there.
///
/// The rename ends every successful build, so this is the common case. The guard drops after the
/// path has moved and finds nothing to remove.
///
/// # Errors
///
/// Fails when the destination is missing or the source remains.
#[test]
fn a_persisted_path_stays() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let from = dir.path().join(".tmp-a");
    let to = dir.path().join("final");
    fs::write(&from, b"x")?;
    Temp::new(from.clone()).persist(&to)?;
    ensure!(to.is_file() && !from.exists(), "the file didn't move");
    Ok(())
}

/// After a failed rename, the drop still removes the path.
///
/// # Errors
///
/// Fails when the temporary path remains or the rename succeeds.
#[test]
fn a_failed_rename_still_removes_the_path() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let from = dir.path().join(".tmp-a");
    fs::write(&from, b"x")?;
    let blocked = dir.path().join("blocked");
    fs::create_dir_all(blocked.join("child"))?;
    ensure!(Temp::new(from.clone()).persist(&blocked).is_err(), "the rename succeeded");
    ensure!(!from.exists(), "the temporary file remains");
    Ok(())
}

/// The sweep removes entries with the temporary prefix and nothing else.
///
/// # Errors
///
/// Fails when a stale entry remains or another entry goes.
#[test]
fn the_sweep_removes_only_temporary_entries() -> Result<()> {
    let dir = tempfile::tempdir()?;
    fs::create_dir_all(dir.path().join(format!("{PREFIX}one/inner")))?;
    fs::write(dir.path().join(format!("{PREFIX}two")), b"x")?;
    fs::write(dir.path().join("keep"), b"x")?;
    ensure!(sweep_stale(dir.path()).is_empty(), "the sweep reported a failure");
    let mut left: Vec<String> = fs::read_dir(dir.path())?
        .map(|entry| entry.map(|found| found.file_name().to_string_lossy().into_owned()))
        .collect::<Result<_, _>>()?;
    left.sort();
    ensure!(left == ["keep"], "left {left:?}");
    Ok(())
}
