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

#[cfg(test)]
mod tests;

use std::fs::{self, DirBuilder, File, OpenOptions, Permissions, TryLockError};
use std::os::unix::fs::{DirBuilderExt as _, MetadataExt as _, OpenOptionsExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};
use std::{env, io};

use anyhow::{Context as _, Result, bail};
use rustix::fs::OFlags;
use rustix::process;

/// The state root directory, held open by its lock.
#[derive(Debug)]
pub struct Root {
    /// Dropping the file releases the lock.
    _lock: File,
    /// The state root's path.
    path: PathBuf,
}

/// What a sweep removed.
#[derive(Debug, Default)]
pub struct Swept {
    /// How many entries the sweep found in the state root's directories.
    pub dirs: usize,
    /// Directories the sweep couldn't remove. The sweep goes on without them.
    pub failures: Vec<String>,
}

impl Root {
    /// The directory with the socket mediators dial.
    #[must_use]
    pub fn hostd(&self) -> PathBuf {
        self.path.join("hostd")
    }

    /// Creates (or reuses) the state root and takes its lock.
    ///
    /// # Errors
    ///
    /// Returns an error if creating the state root fails or it isn't a private directory owned by
    /// this user, or if another avirthostd holds the lock.
    pub fn open() -> Result<Self> {
        Self::open_in(&env::temp_dir())
    }

    /// Creates (or reuses) the state root under `base` and takes its lock.
    ///
    /// # Errors
    ///
    /// Returns an error if creating the state root fails or it isn't a private directory owned by
    /// this user, or if another avirthostd holds the lock.
    fn open_in(base: &Path) -> Result<Self> {
        let path = base.join(format!("avirt-{}", process::getuid().as_raw()));
        private_dir(&path)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .custom_flags(OFlags::NOFOLLOW.bits().cast_signed())
            .open(path.join("lock"))
            .context("open the state root's lock file")?;
        match lock.try_lock() {
            Ok(()) => Ok(Self { _lock: lock, path }),
            Err(TryLockError::WouldBlock) => {
                bail!("another avirthostd is already running (it holds {}); stop it first", path.join("lock").display())
            }
            Err(TryLockError::Error(err)) => Err(err).context("lock the state root"),
        }
    }

    /// Where per-launch temp files live.
    #[must_use]
    pub fn scratch(&self) -> PathBuf {
        self.path.join("scratch")
    }

    /// Removes what a previous avirthostd left behind and recreates the empty layout.
    ///
    /// The lock guarantees nobody else is running, so everything it finds is stale. The sweep
    /// never writes anything back to the host (it deletes a Codex home's `auth.json`). It reports
    /// any directory it can't remove and continues.
    #[must_use]
    pub fn sweep(&self) -> Swept {
        let mut swept = Swept::default();
        for dir in [self.workloads(), self.scratch(), self.hostd()] {
            swept.dirs = swept.dirs.saturating_add(fs::read_dir(&dir).map_or(0, Iterator::count));
            if let Err(err) = remove_tree(&dir) {
                swept.failures.push(format!("{}: {err}", dir.display()));
            }
            if let Err(err) = private_dir(&dir) {
                swept.failures.push(format!("{}: {err}", dir.display()));
            }
        }
        swept
    }

    /// Where per-workload directories live.
    #[must_use]
    pub fn workloads(&self) -> PathBuf {
        self.path.join("workloads")
    }
}

/// Makes a tree removable by its owner with directory mode 0700 and file mode 0600.
///
/// Never follows a link. An agent with ownership of a mounted directory can strip its own
/// permissions (`chmod 0000`), which would make `remove_dir_all` fail and leave an `auth.json`
/// copy behind.
pub fn make_removable(path: &Path) {
    let Ok(meta) = fs::symlink_metadata(path) else { return };
    if meta.is_dir() {
        drop(fs::set_permissions(path, Permissions::from_mode(0o700)));
        for entry in fs::read_dir(path).into_iter().flatten().flatten() {
            make_removable(&entry.path());
        }
    } else if meta.is_file() {
        drop(fs::set_permissions(path, Permissions::from_mode(0o600)));
    } else {
        // Leave a symlink or special file alone, and never follow it.
    }
}

/// Opens (creating it if needed) a private directory owned by this user.
///
/// Refuses a symlink (which `create_dir_all` would accept and `metadata` would follow) and
/// anyone else's directory.
///
/// # Errors
///
/// Returns an error if creating the directory fails, it isn't a plain directory, it isn't owned by
/// this user, or its permissions can't be set.
pub fn private_dir(path: &Path) -> Result<()> {
    DirBuilder::new().recursive(true).mode(0o700).create(path)?;
    let meta = fs::symlink_metadata(path)?;
    if !meta.file_type().is_dir() {
        bail!("{} is not a plain directory (a symlink or a file is in the way)", path.display());
    }
    if meta.uid() != process::getuid().as_raw() {
        bail!("{} is not owned by this user", path.display());
    }
    fs::set_permissions(path, Permissions::from_mode(0o700))?;
    Ok(())
}

/// `remove_dir_all` after [`make_removable`].
///
/// # Errors
///
/// Returns an error if removing the tree fails. A path that doesn't exist counts as removed.
pub fn remove_tree(path: &Path) -> io::Result<()> {
    make_removable(path);
    match fs::remove_dir_all(path) {
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}
