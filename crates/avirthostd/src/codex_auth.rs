//! Codex keeps its login in `auth.json`. `ChatGPT` logins carry a refresh token that rotates when
//! Codex refreshes, so a container that refreshes can invalidate the host's copy. Each actor
//! gets a private copy in a temp `CODEX_HOME`; afterwards a refreshed copy is written back.
//!
//! The agent has write access to the mounted directory, so everything read back from it is
//! untrusted. No function here logs or returns text that may quote file contents.

use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use avirt_core::hostd::{Cleanup, Prepared, Refused};
use avirt_core::state::remove_tree;
use base64::Engine as _;
use chrono::{DateTime, FixedOffset};
use serde_json::{Value, json};
use tempfile::TempDir;

/// Upper bound on the size of an actor's `auth.json`; real files are a few KiB.
const MAX_AUTH_BYTES: u64 = 1 << 20;

pub struct HostAuth {
    path: PathBuf,
}

/// How far into the future a `last_refresh` may be before it is treated as forged.
const MAX_CLOCK_SKEW: chrono::Duration = chrono::Duration::minutes(5);

/// Who the credentials belong to: the stable fields only. They are compared, never logged.
#[derive(Debug, Default, PartialEq, Eq)]
struct Identity {
    account_id: Option<String>,
    /// From the `id_token`'s payload, which is decoded but not verified: it only has to match
    /// what the seed said.
    sub: Option<String>,
    email: Option<String>,
}

/// The parts of an `auth.json` the write-back compares.
#[derive(Debug, PartialEq)]
struct Meta {
    keys: BTreeSet<String>,
    auth_mode: Option<Value>,
    last_refresh: Option<DateTime<FixedOffset>>,
    identity: Identity,
}

fn claim(payload: &Value, name: &str) -> Option<String> {
    payload.get(name).and_then(Value::as_str).map(str::to_owned)
}

fn parse(bytes: &[u8]) -> Option<Meta> {
    // A serde error message can quote the input, so it is dropped here.
    let v: Value = serde_json::from_slice(bytes).ok()?;
    let object = v.as_object()?;
    let tokens = v.get("tokens");
    let payload = tokens
        .and_then(|t| t.get("id_token"))
        .and_then(Value::as_str)
        .and_then(|jwt| jwt.split('.').nth(1))
        .and_then(|part| base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(part.trim_end_matches('=')).ok())
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .unwrap_or(Value::Null);
    Some(Meta {
        keys: object.keys().cloned().collect(),
        auth_mode: v.get("auth_mode").cloned(),
        last_refresh: v.get("last_refresh").and_then(Value::as_str).and_then(|t| DateTime::parse_from_rfc3339(t).ok()),
        identity: Identity {
            account_id: tokens.and_then(|t| claim(t, "account_id")),
            sub: claim(&payload, "sub"),
            email: claim(&payload, "email"),
        },
    })
}

impl HostAuth {
    /// `$CODEX_HOST_HOME/auth.json`, defaulting to `~/.codex/auth.json`.
    pub fn from_env() -> Result<Self> {
        let dir = match std::env::var_os("CODEX_HOST_HOME") {
            Some(dir) => PathBuf::from(dir),
            None => PathBuf::from(std::env::var_os("HOME").context("HOME is not set")?).join(".codex"),
        };
        Ok(Self { path: dir.join("auth.json") })
    }

    /// Reads the host credentials now and seeds a private `CODEX_HOME` with them: the directory
    /// is 0700 and the file 0600, and it is the only thing the container mounts for auth.
    pub fn prepare(&self, scratch: &Path) -> Result<Prepared> {
        let seed = match fs::read(&self.path) {
            Ok(seed) => seed,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Err(Refused(format!(
                    "{} not found; run `codex login` on the host first.",
                    self.path.display()
                ))
                .into());
            }
            Err(e) => return Err(e).with_context(|| format!("read {}", self.path.display())),
        };
        let home = tempfile::Builder::new()
            .prefix("avirt-codex-")
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir_in(scratch)?;
        let mount = format!("{}:/codex-home", home.path().display());
        let actor_file = home.path().join("auth.json");
        let mut file = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&actor_file)?;
        file.write_all(&seed)?;
        drop(file);
        // Owns the directory and the seed; if setup fails after this point, dropping it still
        // writes back and removes the directory, and the seed goes with it.
        let host = self.path.clone();
        let cleanup = Cleanup::new(move || Some(finish(&host, home, &actor_file, &seed)));
        Ok(Prepared {
            args: ["-v", &mount, "-e", "INITIAL_AGENT_MODE", "-e", "CODEX_HOME=/codex-home"].map(Into::into).to_vec(),
            inherit: vec!["INITIAL_AGENT_MODE"],
            cleanup,
        })
    }
}

/// Write-back, then removal of the actor directory. Returns the `auth_writeback` log event.
fn finish(host: &Path, home: TempDir, actor_file: &Path, seed: &[u8]) -> Value {
    let result = match write_back(host, actor_file, seed) {
        Ok(reason) => reason.to_owned(),
        Err(err) => format!("failed: {}", err.kind()),
    };
    // The agent owns the directory and may have made parts of it unreadable.
    let path = home.keep();
    if let Err(e) = remove_tree(&path) {
        eprintln!("could not remove {}: {e}", path.display());
    }
    json!({ "event": "auth_writeback", "result": result })
}

/// Reads the actor's `auth.json` without following symlinks and only if it is a small regular
/// file. `Ok(None)` means there is no file.
fn read_actor_file(path: &Path) -> io::Result<Option<Result<Vec<u8>, &'static str>>> {
    // O_NONBLOCK keeps a planted FIFO from hanging the open; O_NOFOLLOW refuses a symlink.
    let opened = OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK).open(path);
    let file = match opened {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) if matches!(e.raw_os_error(), Some(libc::ELOOP | libc::ENXIO | libc::EOPNOTSUPP | libc::EISDIR)) => {
            return Ok(Some(Err("refused: not a regular file")));
        }
        Err(e) => return Err(e),
    };
    if !file.metadata()?.is_file() {
        return Ok(Some(Err("refused: not a regular file")));
    }
    let mut bytes = Vec::new();
    file.take(MAX_AUTH_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_AUTH_BYTES {
        return Ok(Some(Err("refused: file too large")));
    }
    Ok(Some(Ok(bytes)))
}

/// Copies the actor's `auth.json` over the host's only if Codex refreshed it there and it is
/// strictly newer than the host's current file. Returns a fixed reason string.
fn write_back(host: &Path, actor_file: &Path, seed: &[u8]) -> io::Result<&'static str> {
    let after = match read_actor_file(actor_file)? {
        None => return Ok("no auth.json in actor home"),
        Some(Err(reason)) => return Ok(reason),
        Some(Ok(bytes)) => bytes,
    };
    if after == seed {
        return Ok("unchanged");
    }
    let (Some(new), Some(old)) = (parse(&after), parse(seed)) else {
        return Ok("refused: invalid auth.json");
    };
    let Some(new_refresh) = new.last_refresh else { return Ok("refused: invalid auth.json") };
    // The agent can write anything into its copy, so the copy only replaces the host's if it is
    // plausibly the same login, refreshed: same shape, same mode, same account, and not claiming
    // to have been refreshed in the future (which would also freeze every later write-back).
    if new.keys != old.keys {
        return Ok("refused: auth.json keys changed");
    }
    if new.auth_mode != old.auth_mode {
        return Ok("refused: auth_mode changed");
    }
    if new.identity != old.identity {
        return Ok("refused: account changed");
    }
    if new_refresh.to_utc() > chrono::Utc::now() + MAX_CLOCK_SKEW {
        return Ok("refused: last_refresh in the future");
    }

    // One writer at a time across actors and avirthostd processes: the compare and the replace below must
    // not interleave with another actor's.
    let dir = host.parent().unwrap_or_else(|| Path::new("."));
    let lock =
        OpenOptions::new().write(true).create(true).truncate(false).mode(0o600).open(dir.join("auth.json.lock"))?;
    lock.lock()?;

    // Judge the host file as it is now, under the lock: if it is unreadable as an auth file, or
    // the login was switched to another mode meanwhile, leave it alone.
    let current = fs::read(host)?;
    let Some(host_meta) = parse(&current) else { return Ok("refused: host auth.json changed") };
    if host_meta.auth_mode != new.auth_mode || host_meta.identity != new.identity {
        return Ok("refused: host auth.json changed");
    }
    if host_meta.last_refresh.is_some_and(|host_refresh| new_refresh <= host_refresh) {
        return Ok("host newer");
    }

    // Keep only the first backup. It is written complete to a temp file and then hard-linked
    // into place, so a failure partway never leaves a truncated backup that blocks a real one.
    let backup = dir.join("auth.json.pre-avirt-backup");
    let tmp = write_private_temp(dir, &current)?;
    let linked = fs::hard_link(&tmp, &backup);
    let _ = fs::remove_file(&tmp);
    match linked {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }
    replace_atomically(dir, host, &after)?;
    Ok("wrote refreshed auth.json back to host")
}

/// Writes `contents` to a uniquely named exclusive-create 0600 temp file in `dir` and fsyncs it.
fn write_private_temp(dir: &Path, contents: &[u8]) -> io::Result<PathBuf> {
    let tmp = dir.join(format!("auth.json.avirt-tmp-{:016x}", rand::random::<u64>()));
    let write = || -> io::Result<()> {
        let mut file = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&tmp)?;
        file.write_all(contents)?;
        file.sync_all()
    };
    write().map(|()| tmp.clone()).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}

/// Replaces `host` with `contents` through a temp file and a rename.
fn replace_atomically(dir: &Path, host: &Path, contents: &[u8]) -> io::Result<()> {
    let tmp = write_private_temp(dir, contents)?;
    fs::rename(&tmp, host).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })?;
    File::open(dir)?.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auth(mode: &str, refresh: &str) -> Vec<u8> {
        json!({ "auth_mode": mode, "last_refresh": refresh, "tokens": { "refresh_token": "secret" } })
            .to_string()
            .into_bytes()
    }

    struct Fixture {
        _dir: TempDir,
        host: PathBuf,
        actor: PathBuf,
    }

    fn fixture(host_contents: &[u8], actor_contents: &[u8]) -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path().join("auth.json");
        let actor = dir.path().join("actor-auth.json");
        fs::write(&host, host_contents).unwrap();
        fs::write(&actor, actor_contents).unwrap();
        Fixture { _dir: dir, host, actor }
    }

    const OLD: &str = "2026-10-01T00:00:00Z";
    const NEW: &str = "2026-10-02T00:00:00Z";
    const NEWER: &str = "2026-10-03T00:00:00Z";

    #[test]
    fn actor_home_is_private_and_removed_by_cleanup() {
        let seed = auth("chatgpt", OLD);
        let f = fixture(&seed, &seed);
        let mut prepared = HostAuth { path: f.host.clone() }.prepare(f.host.parent().unwrap()).unwrap();
        let mount = prepared.args[1].to_str().unwrap().to_owned();
        let dir = PathBuf::from(mount.strip_suffix(":/codex-home").unwrap());
        assert_eq!(fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
        assert_eq!(fs::metadata(dir.join("auth.json")).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(fs::read(dir.join("auth.json")).unwrap(), seed);
        let note = prepared.cleanup.run().unwrap();
        assert_eq!(note["result"], "unchanged");
        assert!(!dir.exists());
    }

    #[test]
    fn newer_actor_copy_replaces_host_and_backup_is_kept_once() {
        let seed = auth("chatgpt", OLD);
        let f = fixture(&seed, &auth("chatgpt", NEW));
        assert_eq!(write_back(&f.host, &f.actor, &seed).unwrap(), "wrote refreshed auth.json back to host");
        assert_eq!(fs::read(&f.host).unwrap(), auth("chatgpt", NEW));
        let backup = f.host.with_file_name("auth.json.pre-avirt-backup");
        assert_eq!(fs::read(&backup).unwrap(), seed);

        // A second rotation must not overwrite the first backup.
        fs::write(&f.actor, auth("chatgpt", NEWER)).unwrap();
        write_back(&f.host, &f.actor, &seed).unwrap();
        assert_eq!(fs::read(&f.host).unwrap(), auth("chatgpt", NEWER));
        assert_eq!(fs::read(&backup).unwrap(), seed);
    }

    #[test]
    fn host_that_moved_on_is_not_clobbered() {
        let seed = auth("chatgpt", OLD);
        let f = fixture(&auth("chatgpt", NEWER), &auth("chatgpt", NEW));
        assert_eq!(write_back(&f.host, &f.actor, &seed).unwrap(), "host newer");
        assert_eq!(fs::read(&f.host).unwrap(), auth("chatgpt", NEWER));
    }

    #[test]
    fn unchanged_and_invalid_copies_are_left_alone() {
        let seed = auth("chatgpt", OLD);
        let f = fixture(&seed, &seed);
        assert_eq!(write_back(&f.host, &f.actor, &seed).unwrap(), "unchanged");
        fs::write(&f.actor, b"not json with secret").unwrap();
        assert_eq!(write_back(&f.host, &f.actor, &seed).unwrap(), "refused: invalid auth.json");
        fs::write(&f.actor, auth("apikey", NEW)).unwrap();
        assert_eq!(write_back(&f.host, &f.actor, &seed).unwrap(), "refused: auth_mode changed");
        assert_eq!(fs::read(&f.host).unwrap(), seed);
    }

    #[test]
    fn host_switched_login_mode_is_not_clobbered() {
        let seed = auth("chatgpt", OLD);
        let f = fixture(&auth("apikey", OLD), &auth("chatgpt", NEW));
        assert_eq!(write_back(&f.host, &f.actor, &seed).unwrap(), "refused: host auth.json changed");
        assert_eq!(fs::read(&f.host).unwrap(), auth("apikey", OLD));
        assert!(!f.host.with_file_name("auth.json.pre-avirt-backup").exists());
    }

    #[test]
    fn symlink_and_oversize_actor_files_are_refused() {
        let seed = auth("chatgpt", OLD);
        let f = fixture(&seed, &auth("chatgpt", NEW));
        fs::remove_file(&f.actor).unwrap();
        std::os::unix::fs::symlink(&f.host, &f.actor).unwrap();
        assert_eq!(write_back(&f.host, &f.actor, &seed).unwrap(), "refused: not a regular file");
        fs::remove_file(&f.actor).unwrap();
        fs::write(&f.actor, vec![b' '; 2 << 20]).unwrap();
        assert_eq!(write_back(&f.host, &f.actor, &seed).unwrap(), "refused: file too large");
        assert_eq!(fs::read(&f.host).unwrap(), seed);
    }

    fn jwt(sub: &str, email: &str) -> String {
        let enc = |v: &Value| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(v.to_string());
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

    #[test]
    fn a_refresh_that_keeps_the_identity_is_accepted() {
        let seed = login(OLD, "acct-1", "user-1");
        let f = fixture(&seed, &login(NEW, "acct-1", "user-1"));
        assert_eq!(write_back(&f.host, &f.actor, &seed).unwrap(), "wrote refreshed auth.json back to host");
    }

    #[test]
    fn a_forged_future_last_refresh_is_refused() {
        let seed = login(OLD, "acct-1", "user-1");
        let f = fixture(&seed, &login("9999-12-31T23:59:59Z", "acct-1", "user-1"));
        assert_eq!(write_back(&f.host, &f.actor, &seed).unwrap(), "refused: last_refresh in the future");
        assert_eq!(fs::read(&f.host).unwrap(), seed);
        // Slightly ahead of the clock is tolerated.
        let soon = (chrono::Utc::now() + chrono::Duration::minutes(2)).to_rfc3339();
        fs::write(&f.actor, login(&soon, "acct-1", "user-1")).unwrap();
        assert_eq!(write_back(&f.host, &f.actor, &seed).unwrap(), "wrote refreshed auth.json back to host");
    }

    #[test]
    fn another_account_is_refused() {
        let seed = login(OLD, "acct-1", "user-1");
        for other in [login(NEW, "acct-2", "user-1"), login(NEW, "acct-1", "user-2")] {
            let f = fixture(&seed, &other);
            assert_eq!(write_back(&f.host, &f.actor, &seed).unwrap(), "refused: account changed");
            assert_eq!(fs::read(&f.host).unwrap(), seed);
        }
    }

    #[test]
    fn a_changed_key_set_is_refused() {
        let seed = login(OLD, "acct-1", "user-1");
        let mut changed: Value = serde_json::from_slice(&login(NEW, "acct-1", "user-1")).unwrap();
        changed["extra"] = json!("x");
        let f = fixture(&seed, changed.to_string().as_bytes());
        assert_eq!(write_back(&f.host, &f.actor, &seed).unwrap(), "refused: auth.json keys changed");
        let mut dropped: Value = serde_json::from_slice(&login(NEW, "acct-1", "user-1")).unwrap();
        dropped.as_object_mut().unwrap().remove("OPENAI_API_KEY");
        fs::write(&f.actor, dropped.to_string()).unwrap();
        assert_eq!(write_back(&f.host, &f.actor, &seed).unwrap(), "refused: auth.json keys changed");
    }

    #[test]
    fn cleanup_removes_a_home_the_agent_locked_down() {
        let seed = login(OLD, "acct-1", "user-1");
        let f = fixture(&seed, &seed);
        let scratch = f.host.parent().unwrap().to_path_buf();
        let mut prepared = HostAuth { path: f.host.clone() }.prepare(&scratch).unwrap();
        let mount = prepared.args[1].to_str().unwrap().to_owned();
        let dir = PathBuf::from(mount.strip_suffix(":/codex-home").unwrap());
        let trap = dir.join("lock");
        fs::create_dir(&trap).unwrap();
        fs::copy(dir.join("auth.json"), trap.join("auth.json")).unwrap();
        fs::set_permissions(&trap, fs::Permissions::from_mode(0o000)).unwrap();
        prepared.cleanup.run().unwrap();
        assert!(!dir.exists());
    }
}
