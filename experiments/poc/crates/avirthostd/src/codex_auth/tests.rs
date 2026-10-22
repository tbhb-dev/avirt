//! Unit tests for the Codex `auth.json` seed and write-back.

use std::fs;
use std::os::unix::fs as unix_fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, ensure};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{Duration, Utc};
use serde_json::{Value, json};
use tempfile::TempDir;

use super::{HostAuth, Prepared, write_back};

/// An early `last_refresh`.
const OLD: &str = "2026-10-01T00:00:00Z";
/// A `last_refresh` after [`OLD`].
const NEW: &str = "2026-10-02T00:00:00Z";
/// A `last_refresh` after [`NEW`].
const NEWER: &str = "2026-10-03T00:00:00Z";

/// A host `auth.json` and a workload copy in one temp directory.
struct Fixture {
    /// The temp directory that contains both files.
    dir: TempDir,
    /// The host's `auth.json`.
    host: PathBuf,
    /// The workload's copy.
    workload: PathBuf,
}

impl Fixture {
    /// Returns the directory with both files, for use as the scratch directory.
    fn scratch(&self) -> &Path {
        self.dir.path()
    }
}

/// The write-back rejects a copy that adds or drops a top-level key.
///
/// A changed key set means the workload rewrote more than the tokens, so the host file must remain unchanged.
///
/// # Errors
///
/// Fails when a copy with an added or dropped key reaches the host file.
#[test]
fn a_changed_key_set_is_refused() -> Result<()> {
    let seed = login(OLD, "acct-1", "user-1");
    let mut changed: Value = serde_json::from_slice(&login(NEW, "acct-1", "user-1"))?;
    changed.as_object_mut().context("auth.json is an object")?.insert("extra".into(), json!("x"));
    let case = fixture(&seed, changed.to_string().as_bytes())?;
    ensure!(write_back(&case.host, &case.workload, &seed)? == "refused: auth.json keys changed");
    let mut dropped: Value = serde_json::from_slice(&login(NEW, "acct-1", "user-1"))?;
    dropped.as_object_mut().context("auth.json is an object")?.remove("OPENAI_API_KEY");
    fs::write(&case.workload, dropped.to_string())?;
    ensure!(write_back(&case.host, &case.workload, &seed)? == "refused: auth.json keys changed");
    Ok(())
}

/// Rejects a `last_refresh` far in the future and accepts one a little ahead.
///
/// # Errors
///
/// Fails when the write-back takes a forged timestamp or rejects a near-future one.
#[test]
fn a_forged_future_last_refresh_is_refused() -> Result<()> {
    let seed = login(OLD, "acct-1", "user-1");
    let case = fixture(&seed, &login("9999-12-31T23:59:59Z", "acct-1", "user-1"))?;
    ensure!(write_back(&case.host, &case.workload, &seed)? == "refused: last_refresh in the future");
    ensure!(fs::read(&case.host)? == seed);
    // Accept a time slightly ahead of the clock.
    let soon = (Utc::now() + Duration::minutes(2)).to_rfc3339();
    fs::write(&case.workload, login(&soon, "acct-1", "user-1"))?;
    ensure!(write_back(&case.host, &case.workload, &seed)? == "wrote refreshed auth.json back to host");
    Ok(())
}

/// A refresh from the same account updates the host file.
///
/// # Errors
///
/// Fails when the same account's refresh doesn't update the host file.
#[test]
fn a_refresh_that_keeps_the_identity_is_accepted() -> Result<()> {
    let seed = login(OLD, "acct-1", "user-1");
    let case = fixture(&seed, &login(NEW, "acct-1", "user-1"))?;
    ensure!(write_back(&case.host, &case.workload, &seed)? == "wrote refreshed auth.json back to host");
    Ok(())
}

/// Another account id or subject makes the write-back reject the copy.
///
/// # Errors
///
/// Fails when a copy for a different account or subject reaches the host file.
#[test]
fn another_account_is_refused() -> Result<()> {
    let seed = login(OLD, "acct-1", "user-1");
    for other in [login(NEW, "acct-2", "user-1"), login(NEW, "acct-1", "user-2")] {
        let case = fixture(&seed, &other)?;
        ensure!(write_back(&case.host, &case.workload, &seed)? == "refused: account changed");
        ensure!(fs::read(&case.host)? == seed);
    }
    Ok(())
}

/// Builds a minimal `auth.json` from a login mode and a refresh time.
fn auth(mode: &str, refresh: &str) -> Vec<u8> {
    json!({ "auth_mode": mode, "last_refresh": refresh, "tokens": { "refresh_token": "secret" } })
        .to_string()
        .into_bytes()
}

/// The directory that `prepared` mounts as the workload's Codex home.
///
/// # Errors
///
/// Fails when `prepared` has no share at `/codex-home`.
fn home_dir(prepared: &Prepared) -> Result<PathBuf> {
    let share = prepared.shares.iter().find(|share| share.guest == "/codex-home");
    Ok(share.context("no directory is shared at /codex-home")?.host.clone())
}

/// The cleanup removes a subdirectory the agent made unreadable.
///
/// # Errors
///
/// Fails when the cleanup leaves the locked-down home behind.
#[test]
fn cleanup_removes_a_home_the_agent_locked_down() -> Result<()> {
    let seed = login(OLD, "acct-1", "user-1");
    let case = fixture(&seed, &seed)?;
    let mut prepared = HostAuth { path: case.host.clone() }.prepare(case.scratch())?;
    let dir = home_dir(&prepared)?;
    let trap = dir.join("lock");
    fs::create_dir_all(&trap)?;
    fs::copy(dir.join("auth.json"), trap.join("auth.json"))?;
    fs::set_permissions(&trap, fs::Permissions::from_mode(0o000))?;
    prepared.cleanup.run().context("the cleanup did not run")?;
    ensure!(!dir.exists());
    Ok(())
}

/// Writes the host and workload files into a fresh temp directory.
///
/// # Errors
///
/// Fails when creating the directory or a file fails.
fn fixture(host_contents: &[u8], workload_contents: &[u8]) -> Result<Fixture> {
    let dir = tempfile::tempdir()?;
    let host = dir.path().join("auth.json");
    let workload = dir.path().join("workload-auth.json");
    fs::write(&host, host_contents)?;
    fs::write(&workload, workload_contents)?;
    Ok(Fixture { dir, host, workload })
}

/// When the host switched login mode, its file is unchanged and no backup appears.
///
/// # Errors
///
/// Fails when a host that switched login mode loses its file or gains a backup.
#[test]
fn host_switched_login_mode_is_not_clobbered() -> Result<()> {
    let seed = auth("chatgpt", OLD);
    let case = fixture(&auth("apikey", OLD), &auth("chatgpt", NEW))?;
    ensure!(write_back(&case.host, &case.workload, &seed)? == "refused: host auth.json changed");
    ensure!(fs::read(&case.host)? == auth("apikey", OLD));
    ensure!(!case.host.with_file_name("auth.json.pre-avirt-backup").exists());
    Ok(())
}

/// A host file newer than the workload copy remains unchanged.
///
/// # Errors
///
/// Fails when a newer host file loses its contents to an older copy.
#[test]
fn host_that_moved_on_is_not_clobbered() -> Result<()> {
    let seed = auth("chatgpt", OLD);
    let case = fixture(&auth("chatgpt", NEWER), &auth("chatgpt", NEW))?;
    ensure!(write_back(&case.host, &case.workload, &seed)? == "host newer");
    ensure!(fs::read(&case.host)? == auth("chatgpt", NEWER));
    Ok(())
}

/// Builds an `id_token` whose payload has `sub` and `email`.
fn jwt(sub: &str, email: &str) -> String {
    let enc = |value: &Value| URL_SAFE_NO_PAD.encode(value.to_string());
    format!("{}.{}.sig", enc(&json!({ "alg": "none" })), enc(&json!({ "sub": sub, "email": email })))
}

/// An `auth.json` shaped like Codex's, for one account.
fn login(refresh: &str, account: &str, sub: &str) -> Vec<u8> {
    json!({
        "OPENAI_API_KEY": null,
        "auth_mode": "chatgpt",
        "last_refresh": refresh,
        "tokens": { "account_id": account, "id_token": jwt(sub, "a@example.com"), "refresh_token": "r", "access_token": "a" },
    })
    .to_string()
    .into_bytes()
}

/// A newer workload copy replaces the host file, and the first backup remains.
///
/// # Errors
///
/// Fails when the host file misses the update or the first backup changes.
#[test]
fn newer_workload_copy_replaces_host_and_backup_is_kept_once() -> Result<()> {
    let seed = auth("chatgpt", OLD);
    let case = fixture(&seed, &auth("chatgpt", NEW))?;
    ensure!(write_back(&case.host, &case.workload, &seed)? == "wrote refreshed auth.json back to host");
    ensure!(fs::read(&case.host)? == auth("chatgpt", NEW));
    let backup = case.host.with_file_name("auth.json.pre-avirt-backup");
    ensure!(fs::read(&backup)? == seed);

    // A second rotation must not overwrite the first backup.
    fs::write(&case.workload, auth("chatgpt", NEWER))?;
    write_back(&case.host, &case.workload, &seed)?;
    ensure!(fs::read(&case.host)? == auth("chatgpt", NEWER));
    ensure!(fs::read(&backup)? == seed);
    Ok(())
}

/// The write-back rejects a symlink or an oversize file in the workload home.
///
/// # Errors
///
/// Fails when the write-back copies a symlink or an oversize file to the host.
#[test]
fn symlink_and_oversize_workload_files_are_refused() -> Result<()> {
    let seed = auth("chatgpt", OLD);
    let case = fixture(&seed, &auth("chatgpt", NEW))?;
    fs::remove_file(&case.workload)?;
    unix_fs::symlink(&case.host, &case.workload)?;
    ensure!(write_back(&case.host, &case.workload, &seed)? == "refused: not a regular file");
    fs::remove_file(&case.workload)?;
    fs::write(&case.workload, vec![b' '; 2 << 20])?;
    ensure!(write_back(&case.host, &case.workload, &seed)? == "refused: file too large");
    ensure!(fs::read(&case.host)? == seed);
    Ok(())
}

/// Unchanged, malformed, and mode-changing copies leave the host file alone.
///
/// # Errors
///
/// Fails when an unchanged, malformed or mode-changing copy touches the host file.
#[test]
fn unchanged_and_invalid_copies_are_left_alone() -> Result<()> {
    let seed = auth("chatgpt", OLD);
    let case = fixture(&seed, &seed)?;
    ensure!(write_back(&case.host, &case.workload, &seed)? == "unchanged");
    fs::write(&case.workload, b"not json with secret")?;
    ensure!(write_back(&case.host, &case.workload, &seed)? == "refused: invalid auth.json");
    fs::write(&case.workload, auth("apikey", NEW))?;
    ensure!(write_back(&case.host, &case.workload, &seed)? == "refused: auth_mode changed");
    ensure!(fs::read(&case.host)? == seed);
    Ok(())
}

/// The seeded home is private, and running the cleanup removes it.
///
/// # Errors
///
/// Fails when the home's permissions or its removal differ from the contract.
#[test]
fn workload_home_is_private_and_removed_by_cleanup() -> Result<()> {
    let seed = auth("chatgpt", OLD);
    let case = fixture(&seed, &seed)?;
    let mut prepared = HostAuth { path: case.host.clone() }.prepare(case.scratch())?;
    let dir = home_dir(&prepared)?;
    ensure!(fs::metadata(&dir)?.permissions().mode() & 0o777 == 0o700);
    ensure!(fs::metadata(dir.join("auth.json"))?.permissions().mode() & 0o777 == 0o600);
    ensure!(fs::read(dir.join("auth.json"))? == seed);
    let note = prepared.cleanup.run().context("the cleanup did not run")?;
    ensure!(note.get("result") == Some(&json!("unchanged")));
    ensure!(!dir.exists());
    Ok(())
}
