//! `avirthostd`'s state root, `$TMPDIR/avirt-<uid>/` (0700): everything a workload leaves on disk
//! lives under it. If avirthostd exits without cleanup, the next instance can remove what it left.
//!
//! ```text
//! lock            held (flock) for avirthostd's lifetime; a second avirthostd refuses to start
//! hostd/         the socket mediators dial
//! workloads/<id>/  per-workload sockets
//! scratch/        per-launch temp files, such as a Codex home holding an `auth.json` copy
//! ```
//!
//! Mediators aren't tracked here: they exit when avirthostd's end of their stdin closes.

use std::fs::{self, File, OpenOptions, Permissions};
use std::io;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde_json::Value;

/// Label put on every container avirthostd starts. A sweep uses it to find them without guessing at names.
pub const CONTAINER_LABEL: &str = "avirt=1";
/// Marks the container as a workload.
pub const WORKLOAD_LABEL: &str = "avirt.role=workload";

#[derive(Debug)]
pub struct StateRoot {
    path: PathBuf,
    /// Dropping the file releases the lock.
    _lock: File,
}

/// What a sweep removed.
#[derive(Debug, Default)]
pub struct Swept {
    pub containers: Vec<String>,
    pub dirs: usize,
    /// Directories the sweep couldn't remove. The sweep goes on without them.
    pub failures: Vec<String>,
}

/// Opens (creating it if needed) a private directory owned by this user, refusing a symlink
/// (which `create_dir_all` would accept and `metadata` would follow) and anyone else's directory.
pub fn private_dir(path: &Path) -> Result<()> {
    fs::DirBuilder::new().recursive(true).mode(0o700).create(path)?;
    let meta = fs::symlink_metadata(path)?;
    if !meta.file_type().is_dir() {
        bail!("{} is not a plain directory (a symlink or a file is in the way)", path.display());
    }
    if meta.uid() != rustix::process::getuid().as_raw() {
        bail!("{} is not owned by this user", path.display());
    }
    fs::set_permissions(path, Permissions::from_mode(0o700))?;
    Ok(())
}

/// Makes a tree removable by its owner with directory mode 0700 and file mode 0600. Never follows a link.
/// An agent with ownership of a mounted directory can strip its own permissions (`chmod 0000`), which
/// would make `remove_dir_all` fail and leave an `auth.json` copy behind.
pub fn make_removable(path: &Path) {
    let Ok(meta) = fs::symlink_metadata(path) else { return };
    let kind = meta.file_type();
    if kind.is_dir() {
        let _ = fs::set_permissions(path, Permissions::from_mode(0o700));
        for entry in fs::read_dir(path).into_iter().flatten().flatten() {
            make_removable(&entry.path());
        }
    } else if kind.is_file() {
        let _ = fs::set_permissions(path, Permissions::from_mode(0o600));
    }
}

/// `remove_dir_all` after [`make_removable`].
pub fn remove_tree(path: &Path) -> io::Result<()> {
    make_removable(path);
    match fs::remove_dir_all(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

impl StateRoot {
    /// Creates (or reuses) the state root and takes its lock. Fails if another avirthostd
    /// holds the lock.
    pub fn open() -> Result<Self> {
        Self::open_in(&std::env::temp_dir())
    }

    fn open_in(base: &Path) -> Result<Self> {
        let path = base.join(format!("avirt-{}", rustix::process::getuid().as_raw()));
        private_dir(&path)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits().cast_signed())
            .open(path.join("lock"))
            .context("open the state root's lock file")?;
        if let Err(err) = lock.try_lock() {
            match err {
                fs::TryLockError::WouldBlock => {
                    bail!(
                        "another avirthostd is already running (it holds {}); stop it first",
                        path.join("lock").display()
                    )
                }
                fs::TryLockError::Error(e) => return Err(e).context("lock the state root"),
            }
        }
        Ok(Self { path, _lock: lock })
    }

    #[must_use]
    pub fn workloads(&self) -> PathBuf {
        self.path.join("workloads")
    }

    #[must_use]
    pub fn scratch(&self) -> PathBuf {
        self.path.join("scratch")
    }

    #[must_use]
    pub fn hostd(&self) -> PathBuf {
        self.path.join("hostd")
    }

    /// Removes what a previous avirthostd left behind and recreates the empty layout. The lock
    /// guarantees nobody else is running, so everything it finds is stale. The sweep never writes anything back
    /// to the host (it deletes a Codex home's `auth.json`). It reports any directory it can't remove
    /// and continues.
    #[must_use]
    pub fn sweep(&self) -> Swept {
        let mut swept = Swept { containers: sweep_containers(), ..Swept::default() };
        for dir in [self.workloads(), self.scratch(), self.hostd()] {
            swept.dirs += fs::read_dir(&dir).map_or(0, Iterator::count);
            if let Err(e) = remove_tree(&dir) {
                swept.failures.push(format!("{}: {e}", dir.display()));
            }
            if let Err(e) = private_dir(&dir) {
                swept.failures.push(format!("{}: {e}", dir.display()));
            }
        }
        swept
    }
}

/// Stops and deletes every container with avirthostd's label, whatever its state.
fn sweep_containers() -> Vec<String> {
    let Ok(out) = Command::new("container").args(["ls", "--all", "--format", "json"]).output() else {
        return Vec::new();
    };
    let Ok(list) = serde_json::from_slice::<Value>(&out.stdout) else { return Vec::new() };
    let (key, value) = CONTAINER_LABEL.split_once('=').expect("key=value");
    let mut removed = Vec::new();
    for container in list.as_array().into_iter().flatten() {
        let config = &container["configuration"];
        if config["labels"][key].as_str() != Some(value) {
            continue;
        }
        let Some(id) = config["id"].as_str() else { continue };
        for args in [["kill", id], ["delete", id]] {
            let _ = Command::new("container").args(args).output();
        }
        removed.push(id.to_owned());
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sweep_empties_the_layout_and_the_lock_is_exclusive() {
        let base = tempfile::tempdir().unwrap();
        let first = StateRoot::open_in(base.path()).unwrap();
        // Refuse a second avirthostd in the same user's temp dir while the first lives.
        assert!(StateRoot::open_in(base.path()).unwrap_err().to_string().contains("already running"));
        let _ = first.sweep();
        fs::create_dir_all(first.scratch().join("avirt-codex-x")).unwrap();
        fs::write(first.scratch().join("avirt-codex-x/auth.json"), "secret").unwrap();
        let swept = first.sweep();
        assert!(swept.dirs >= 1 && swept.failures.is_empty());
        assert!(!first.scratch().join("avirt-codex-x").exists());
        assert!(first.scratch().is_dir());
    }

    #[test]
    fn a_tree_the_agent_made_unreadable_is_still_removed() {
        let base = tempfile::tempdir().unwrap();
        let root = StateRoot::open_in(base.path()).unwrap();
        let trap = root.scratch().join("home/lock");
        fs::create_dir_all(&trap).unwrap();
        fs::write(trap.join("auth.json"), "secret").unwrap();
        fs::set_permissions(trap.join("auth.json"), Permissions::from_mode(0o000)).unwrap();
        fs::set_permissions(&trap, Permissions::from_mode(0o000)).unwrap();
        let swept = root.sweep();
        assert!(swept.failures.is_empty(), "{:?}", swept.failures);
        assert!(!trap.exists());
    }

    #[test]
    fn a_symlinked_state_root_is_refused() {
        let base = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        let link = base.path().join(format!("avirt-{}", rustix::process::getuid().as_raw()));
        std::os::unix::fs::symlink(target.path(), &link).unwrap();
        let err = StateRoot::open_in(base.path()).unwrap_err().to_string();
        assert!(err.contains("not a plain directory"), "{err}");
    }
}
