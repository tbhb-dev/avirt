//! Unit tests for the state root.

use std::fs::{self, Permissions};
use std::os::unix::fs::{self as unix_fs, PermissionsExt as _};

use anyhow::{Result, ensure};
use rustix::process;

use super::Root;

/// A sweep empties the layout while one avirthostd keeps the lock.
///
/// # Errors
///
/// Fails when opening the state root or sweeping it breaks an expectation.
#[test]
fn sweep_empties_the_layout_and_the_lock_is_exclusive() -> Result<()> {
    let base = tempfile::tempdir()?;
    let first = Root::open_in(base.path())?;
    // Refuse a second avirthostd in the same user's temp dir while the first lives.
    let second = Root::open_in(base.path());
    ensure!(second.is_err_and(|err| err.to_string().contains("already running")), "a second root opened");
    drop(first.sweep(&[]));
    fs::create_dir_all(first.scratch().join("avirt-codex-x"))?;
    fs::write(first.scratch().join("avirt-codex-x/auth.json"), "secret")?;
    let swept = first.sweep(&[]);
    ensure!(swept.dirs >= 1 && swept.failures.is_empty(), "the sweep left {:?}", swept.failures);
    ensure!(!first.scratch().join("avirt-codex-x").exists(), "the sweep kept the directory");
    ensure!(first.scratch().is_dir(), "the sweep removed the scratch directory");
    // A sweep leaves the caches, the images, and the container state for the runtime to clean.
    let kept = [first.cache(), first.containers(), first.images()];
    for dir in &kept {
        fs::create_dir_all(dir)?;
    }
    drop(first.sweep(&[]));
    ensure!(kept.iter().all(|dir| dir.is_dir()), "the sweep removed a directory it should leave");
    // A workload directory that the caller names stays, and the others go.
    fs::create_dir_all(first.workloads().join("00000001"))?;
    fs::create_dir_all(first.workloads().join("00000002"))?;
    let named = first.sweep(&["00000001".to_owned()]);
    ensure!(named.dirs == 1 && named.failures.is_empty(), "the sweep reported {named:?}");
    ensure!(first.workloads().join("00000001").is_dir(), "the sweep removed a named directory");
    ensure!(!first.workloads().join("00000002").exists(), "the sweep kept an unnamed directory");
    Ok(())
}

/// A sweep still removes a tree whose permissions the agent stripped.
///
/// # Errors
///
/// Fails when opening the root or sweeping the stripped tree breaks an expectation.
#[test]
fn a_tree_the_agent_made_unreadable_is_still_removed() -> Result<()> {
    let base = tempfile::tempdir()?;
    let root = Root::open_in(base.path())?;
    let trap = root.scratch().join("home/lock");
    fs::create_dir_all(&trap)?;
    fs::write(trap.join("auth.json"), "secret")?;
    fs::set_permissions(trap.join("auth.json"), Permissions::from_mode(0o000))?;
    fs::set_permissions(&trap, Permissions::from_mode(0o000))?;
    let swept = root.sweep(&[]);
    ensure!(swept.failures.is_empty(), "{:?}", swept.failures);
    ensure!(!trap.exists(), "the sweep kept the tree");
    Ok(())
}

/// A symlink where the state root should be makes the call fail.
///
/// # Errors
///
/// Fails when the symlink fixture or the refusal check breaks an expectation.
#[test]
fn a_symlinked_state_root_is_refused() -> Result<()> {
    let base = tempfile::tempdir()?;
    let target = tempfile::tempdir()?;
    let link = base.path().join(format!("avirt-{}", process::getuid().as_raw()));
    unix_fs::symlink(target.path(), &link)?;
    let refusal = Root::open_in(base.path()).err().map(|err| err.to_string()).unwrap_or_default();
    ensure!(refusal.contains("not a plain directory"), "{refusal}");
    Ok(())
}
