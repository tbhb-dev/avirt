//! The launch intent, `workloads/<id>/intent.json`.
//!
//! avirthostd writes it before it starts a helper, so a crash at any point leaves a record that
//! the next startup can reap.

#[cfg(test)]
mod tests;

use core::fmt;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write as _};
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::Path;

/// The intent's filename.
const FILE: &str = "intent.json";
/// Where a write goes before it replaces [`FILE`].
const TEMP: &str = "intent.json.tmp";

/// How far a launch got.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    /// avirthostd confirmed that the helper exited, and the intent no longer records a pid.
    Ended,
    /// Written before the helper starts.
    Intended,
    /// The helper answered `StartWorkload` with `Started`.
    Running,
    /// The helper has started, and the intent records its pid.
    Starting,
    /// avirthostd has begun to stop the helper.
    Stopping,
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Ended => "ended",
            Self::Intended => "intended",
            Self::Running => "running",
            Self::Starting => "starting",
            Self::Stopping => "stopping",
        })
    }
}

/// What avirthostd means to launch.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Intent {
    /// 32 lowercase hexadecimal characters.
    pub incarnation: String,
    /// The pid of the helper, once it starts.
    pub pid: Option<u32>,
    /// The sha256 of the `StartWorkload` with its environment cleared, so no secret is in it.
    pub spec_digest: String,
    /// How far the launch got.
    pub state: State,
    /// The workload's id.
    pub workload: String,
}

impl Intent {
    /// Reads the intent in `dir`. A leftover temporary file from an interrupted write doesn't count.
    ///
    /// # Errors
    ///
    /// Returns an error if the file is missing or damaged.
    pub fn read(dir: &Path) -> io::Result<Self> {
        let bytes = fs::read(dir.join(FILE))?;
        serde_json::from_slice(&bytes).map_err(io::Error::other)
    }

    /// Replaces the intent in `dir` atomically.
    ///
    /// The new content goes into a temporary file, which the write syncs and renames over the old
    /// one before it syncs the directory. A reader sees the old intent or the new one.
    ///
    /// # Errors
    ///
    /// Returns an error if any of those steps fails.
    pub fn write(&self, dir: &Path) -> io::Result<()> {
        let temp = dir.join(TEMP);
        match fs::remove_file(&temp) {
            Err(err) if err.kind() != io::ErrorKind::NotFound => return Err(err),
            Ok(()) | Err(_) => {}
        }
        let mut file = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&temp)?;
        file.write_all(&serde_json::to_vec(self).map_err(io::Error::other)?)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, dir.join(FILE))?;
        File::open(dir)?.sync_all()
    }
}
