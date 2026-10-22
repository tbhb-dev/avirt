//! Temporary paths that remove themselves, and the removal of those that a crash left behind.
//!
//! A crash skips every drop, and the startup sweep removes what it left.

#[cfg(test)]
mod tests;

use crate::state::remove_tree;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The name prefix of every temporary entry in the caches and the image store.
pub const PREFIX: &str = ".tmp-";

/// A temporary file or directory that removes itself on drop, unless it moved into place first.
///
/// The guard removes the path after an error, a failed rename, or a dropped future.
///
/// Only the rename in [`Temp::persist`] ends the guard's duty, because after it the path is a cache entry or a blob that later launches use.
#[derive(Debug)]
pub struct Temp {
    /// The path, until [`Temp::persist`] gives it up.
    path: Option<PathBuf>,
}

impl Temp {
    /// A guard for `path`, which may not exist yet.
    #[must_use]
    pub const fn new(path: PathBuf) -> Self {
        Self { path: Some(path) }
    }

    /// The guarded path.
    #[must_use]
    pub fn path(&self) -> &Path {
        self.path.as_deref().unwrap_or_else(|| Path::new(""))
    }

    /// Moves the path to `to` and keeps it there. When the rename fails, the guard still removes the path on drop.
    ///
    /// # Errors
    ///
    /// Returns an error if the rename fails.
    pub fn persist(mut self, to: &Path) -> io::Result<()> {
        if let Some(from) = &self.path {
            fs::rename(from, to)?;
        }
        self.path = None;
        Ok(())
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        if let Some(path) = &self.path {
            drop(remove(path));
        }
    }
}

/// Removes a file, a link, or a directory tree. A path that doesn't exist counts as removed.
///
/// # Errors
///
/// Returns an error if the removal fails.
fn remove(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => remove_tree(path),
        Ok(_) => fs::remove_file(path),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

/// Removes every `.tmp-*` entry directly under `dir` and returns what it couldn't remove.
///
/// Only the holder of the state root's lock may call it, because a running launch builds in these directories.
#[must_use]
pub fn sweep_stale(dir: &Path) -> Vec<String> {
    let mut failures = Vec::new();
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        if entry.file_name().to_string_lossy().starts_with(PREFIX)
            && let Err(err) = remove(&entry.path())
        {
            failures.push(format!("{}: {err}", entry.path().display()));
        }
    }
    failures
}
