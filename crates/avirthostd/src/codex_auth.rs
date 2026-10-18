//! Codex keeps its login in `auth.json`.
//!
//! `ChatGPT` logins contain a refresh token that rotates when Codex refreshes, so a container
//! that refreshes can invalidate the host's copy. Each workload gets a private copy in a temp
//! `CODEX_HOME`. Afterwards, write back a refreshed copy.
//!
//! The agent has write access to the mounted directory. Treat everything read back from it as
//! untrusted. No function here logs or returns text that may quote file contents.

#[cfg(test)]
mod tests;

use alloc::collections::BTreeSet;
use std::env;
use std::fs::{self, File, OpenOptions, Permissions};
use std::io::{self, Read as _, Write as _};
use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use avirt_core::hostd::{Cleanup, Prepared, Refused};
use avirt_core::state::remove_tree;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Duration, FixedOffset, Utc};
use serde_json::{Value, json};
use tempfile::{Builder, TempDir};

/// How far into the future a `last_refresh` may be before it's treated as forged.
const MAX_CLOCK_SKEW: Duration = Duration::minutes(5);
/// Upper bound on the size of a workload's `auth.json`. Real files are a few KiB.
const MAX_AUTH_BYTES: u64 = 1 << 20;

/// The contents of a workload's `auth.json`, or the reason to refuse it.
type Contents = Result<Vec<u8>, &'static str>;

/// A workload copy that passed the checks that don't need the host file.
struct Candidate {
    /// The workload's `auth.json` as written.
    after: Vec<u8>,
    /// The workload copy's parsed fields.
    meta: Meta,
    /// The workload copy's `last_refresh`.
    refresh: DateTime<FixedOffset>,
}

/// The host's Codex credentials.
#[derive(Debug)]
pub struct HostAuth {
    /// The host's `auth.json`.
    path: PathBuf,
}

/// Who the credentials belong to: the stable fields only. They're compared, never logged.
#[derive(Debug, Default, PartialEq, Eq)]
struct Identity {
    /// The `account_id` token field.
    account_id: Option<String>,
    /// From the `id_token`'s payload.
    email: Option<String>,
    /// From the `id_token`'s payload. The parser decodes it without verification:
    /// it only has to match what the seed said.
    sub: Option<String>,
}

/// The parts of an `auth.json` the write-back compares.
#[derive(Debug, PartialEq)]
struct Meta {
    /// The `auth_mode` value.
    auth_mode: Option<Value>,
    /// Who the credentials belong to.
    identity: Identity,
    /// The top-level keys.
    keys: BTreeSet<String>,
    /// The `last_refresh` timestamp.
    last_refresh: Option<DateTime<FixedOffset>>,
}

impl HostAuth {
    /// `$CODEX_HOST_HOME/auth.json`, defaulting to `~/.codex/auth.json`.
    ///
    /// # Errors
    ///
    /// Returns an error if `CODEX_HOST_HOME` is unset and `HOME` is unset too.
    pub fn from_env() -> Result<Self> {
        let dir = match env::var_os("CODEX_HOST_HOME") {
            Some(dir) => PathBuf::from(dir),
            None => PathBuf::from(env::var_os("HOME").context("HOME is not set")?).join(".codex"),
        };
        Ok(Self { path: dir.join("auth.json") })
    }

    /// Reads the host credentials now and seeds a private `CODEX_HOME` with them: the directory
    /// is 0700 and the file 0600, and it's the only thing the container mounts for auth.
    ///
    /// # Errors
    ///
    /// Returns [`Refused`] if the host has no `auth.json`. Any other error means the host file
    /// is unreadable or the private directory fails to create.
    pub fn prepare(&self, scratch: &Path) -> Result<Prepared> {
        let seed = match fs::read(&self.path) {
            Ok(seed) => seed,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                return Err(Refused(format!(
                    "{} not found; run `codex login` on the host first.",
                    self.path.display()
                ))
                .into());
            }
            Err(err) => return Err(err).with_context(|| format!("read {}", self.path.display())),
        };
        let home =
            Builder::new().prefix("avirt-codex-").permissions(Permissions::from_mode(0o700)).tempdir_in(scratch)?;
        let mount = format!("{}:/codex-home", home.path().display());
        let workload_file = home.path().join("auth.json");
        let mut file = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&workload_file)?;
        file.write_all(&seed)?;
        drop(file);
        // Retains ownership of the directory and the seed. If setup fails after this point, dropping it still
        // writes back and removes the directory, and the seed goes with it.
        let host = self.path.clone();
        let cleanup = Cleanup::new(move || Some(finish(&host, home, &workload_file, &seed)));
        Ok(Prepared {
            args: ["-v", &mount, "-e", "INITIAL_AGENT_MODE", "-e", "CODEX_HOME=/codex-home"].map(Into::into).to_vec(),
            inherit: vec!["INITIAL_AGENT_MODE"],
            cleanup,
        })
    }
}

/// Reads a string claim from a JSON object.
fn claim(payload: &Value, name: &str) -> Option<String> {
    payload.get(name).and_then(Value::as_str).map(str::to_owned)
}

/// Write-back, then removal of the workload directory. Returns the `auth_writeback` log event.
fn finish(host: &Path, home: TempDir, workload_file: &Path, seed: &[u8]) -> Value {
    let result = match write_back(host, workload_file, seed) {
        Ok(reason) => reason.to_owned(),
        Err(err) => format!("failed: {}", err.kind()),
    };
    // The directory belongs to the agent, which may have made parts of it unreadable.
    let path = home.keep();
    if let Err(err) = remove_tree(&path) {
        eprintln!("could not remove {}: {err}", path.display());
    }
    json!({ "event": "auth_writeback", "result": result })
}

/// Refuses a host file that moved on while the workload ran: unreadable as an auth file, a
/// different login mode or account, or already at least as new as the workload's copy.
fn host_refusal(current: &[u8], candidate: &Candidate) -> Option<&'static str> {
    let Some(host_meta) = parse(current) else { return Some("refused: host auth.json changed") };
    if host_meta.auth_mode != candidate.meta.auth_mode || host_meta.identity != candidate.meta.identity {
        return Some("refused: host auth.json changed");
    }
    if host_meta.last_refresh.is_some_and(|host_refresh| candidate.refresh <= host_refresh) {
        return Some("host newer");
    }
    None
}

/// Keeps only the first backup of the host file. It's written complete to a temp file and then
/// hard-linked into place, so a failure partway never leaves a truncated backup that blocks a
/// real one.
///
/// # Errors
///
/// Returns an error if the temp file write fails or the hard link fails for a reason other than
/// an existing backup.
fn keep_first_backup(dir: &Path, current: &[u8]) -> io::Result<()> {
    let backup = dir.join("auth.json.pre-avirt-backup");
    let tmp = write_private_temp(dir, current)?;
    let linked = fs::hard_link(&tmp, &backup);
    remove_quietly(&tmp);
    match linked {
        Err(err) if err.kind() != io::ErrorKind::AlreadyExists => Err(err),
        Ok(()) | Err(_) => Ok(()),
    }
}

/// Parses the fields of an `auth.json` that the write-back compares.
fn parse(bytes: &[u8]) -> Option<Meta> {
    // A serde error message can quote the input, so it's dropped here.
    let value: Value = serde_json::from_slice(bytes).ok()?;
    let object = value.as_object()?;
    let tokens = value.get("tokens");
    let payload = tokens
        .and_then(|section| section.get("id_token"))
        .and_then(Value::as_str)
        .and_then(|jwt| jwt.split('.').nth(1))
        .and_then(|part| URL_SAFE_NO_PAD.decode(part.trim_end_matches('=')).ok())
        .and_then(|decoded| serde_json::from_slice::<Value>(&decoded).ok())
        .unwrap_or(Value::Null);
    Some(Meta {
        keys: object.keys().cloned().collect(),
        auth_mode: value.get("auth_mode").cloned(),
        last_refresh: value
            .get("last_refresh")
            .and_then(Value::as_str)
            .and_then(|stamp| DateTime::parse_from_rfc3339(stamp).ok()),
        identity: Identity {
            account_id: tokens.and_then(|section| claim(section, "account_id")),
            sub: claim(&payload, "sub"),
            email: claim(&payload, "email"),
        },
    })
}

/// Refuses a workload copy unless it plausibly has the same keys, mode, and account as the seed
/// and doesn't claim a future refresh timestamp (which would also freeze every later write-back).
fn plausibility_refusal(new: &Meta, old: &Meta, new_refresh: DateTime<FixedOffset>) -> Option<&'static str> {
    if new.keys != old.keys {
        return Some("refused: auth.json keys changed");
    }
    if new.auth_mode != old.auth_mode {
        return Some("refused: auth_mode changed");
    }
    if new.identity != old.identity {
        return Some("refused: account changed");
    }
    // An overflowing limit is beyond every timestamp, so nothing is in the future.
    if Utc::now().checked_add_signed(MAX_CLOCK_SKEW).is_some_and(|limit| new_refresh.to_utc() > limit) {
        return Some("refused: last_refresh in the future");
    }
    None
}

/// Reads the workload's `auth.json` without following symlinks and only if it's a small regular
/// file. `Ok(None)` means there is no file.
///
/// # Errors
///
/// Returns an error if opening or reading the file fails for a reason other than a missing file
/// or a file that isn't a regular file.
fn read_workload_file(path: &Path) -> io::Result<Option<Contents>> {
    // O_NONBLOCK keeps a planted FIFO from hanging the open. O_NOFOLLOW refuses a symlink.
    let opened = OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK).open(path);
    let file = match opened {
        Ok(file) => file,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) if matches!(err.raw_os_error(), Some(libc::ELOOP | libc::ENXIO | libc::EOPNOTSUPP | libc::EISDIR)) => {
            return Ok(Some(Err("refused: not a regular file")));
        }
        Err(err) => return Err(err),
    };
    if !file.metadata()?.is_file() {
        return Ok(Some(Err("refused: not a regular file")));
    }
    let mut bytes = Vec::new();
    file.take(MAX_AUTH_BYTES + 1).read_to_end(&mut bytes)?;
    if !u64::try_from(bytes.len()).is_ok_and(|len| len <= MAX_AUTH_BYTES) {
        return Ok(Some(Err("refused: file too large")));
    }
    Ok(Some(Ok(bytes)))
}

/// Replaces `host` with `contents` through a temp file and a rename.
///
/// # Errors
///
/// Returns an error if writing, renaming, or syncing the temp file fails.
fn replace_atomically(dir: &Path, host: &Path, contents: &[u8]) -> io::Result<()> {
    let tmp = write_private_temp(dir, contents)?;
    fs::rename(&tmp, host).inspect_err(|_| remove_quietly(&tmp))?;
    File::open(dir)?.sync_all()
}

/// Removes `path` on a best-effort basis, because the caller is already returning another error.
fn remove_quietly(path: &Path) {
    drop(fs::remove_file(path));
}

/// Reads the workload's copy and applies the checks that don't need the host file. The `Err`
/// side carries the fixed reason the copy isn't written back.
///
/// # Errors
///
/// Returns an error if opening or reading the workload file fails for a reason other than a
/// missing file or a file that isn't a regular file.
fn screen(workload_file: &Path, seed: &[u8]) -> io::Result<Result<Candidate, &'static str>> {
    let after = match read_workload_file(workload_file)? {
        None => return Ok(Err("no auth.json in workload home")),
        Some(Err(reason)) => return Ok(Err(reason)),
        Some(Ok(bytes)) => bytes,
    };
    if after == seed {
        return Ok(Err("unchanged"));
    }
    let (Some(meta), Some(old)) = (parse(&after), parse(seed)) else {
        return Ok(Err("refused: invalid auth.json"));
    };
    let Some(refresh) = meta.last_refresh else { return Ok(Err("refused: invalid auth.json")) };
    // The agent can write anything into its copy, so the refreshed login has to pass
    // these checks before the write-back replaces the host's copy.
    Ok(plausibility_refusal(&meta, &old, refresh).map_or(Ok(Candidate { after, meta, refresh }), Err))
}

/// Copies the workload's `auth.json` over the host's only if Codex refreshed it there and it's
/// strictly newer than the host's current file. Returns a fixed reason string.
///
/// # Errors
///
/// Returns an error if the lock, the host file, the backup, or the replacement fails on disk.
fn write_back(host: &Path, workload_file: &Path, seed: &[u8]) -> io::Result<&'static str> {
    let candidate = match screen(workload_file, seed)? {
        Ok(candidate) => candidate,
        Err(reason) => return Ok(reason),
    };

    // One writer at a time across workloads and avirthostd processes: the compare and the replace below must
    // not interleave with another workload's.
    let dir = host.parent().unwrap_or_else(|| Path::new("."));
    let lock =
        OpenOptions::new().write(true).create(true).truncate(false).mode(0o600).open(dir.join("auth.json.lock"))?;
    lock.lock()?;

    // Judge the current host file under the lock: if it's unreadable as an auth file, or
    // the login now uses another mode, leave it alone.
    let current = fs::read(host)?;
    if let Some(reason) = host_refusal(&current, &candidate) {
        return Ok(reason);
    }

    keep_first_backup(dir, &current)?;
    replace_atomically(dir, host, &candidate.after)?;
    Ok("wrote refreshed auth.json back to host")
}

/// Writes `contents` to a uniquely named exclusive-create 0600 temp file in `dir` and fsyncs it.
///
/// # Errors
///
/// Returns an error if creating, writing, or syncing the file fails. The function removes the partial file.
fn write_private_temp(dir: &Path, contents: &[u8]) -> io::Result<PathBuf> {
    let tmp = dir.join(format!("auth.json.avirt-tmp-{:016x}", rand::random::<u64>()));
    let write = || -> io::Result<()> {
        let mut file = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&tmp)?;
        file.write_all(contents)?;
        file.sync_all()
    };
    write().map(|()| tmp.clone()).inspect_err(|_| remove_quietly(&tmp))
}
