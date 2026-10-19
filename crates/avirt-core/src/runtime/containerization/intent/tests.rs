//! Unit tests for the launch intent.

use super::{FILE, Intent, State, TEMP};
use anyhow::{Result, ensure};
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt as _;

/// An intent for workload `1a2b3c4d` in `state`.
fn intent(state: State, pid: Option<u32>) -> Intent {
    Intent {
        incarnation: "000102030405060708090a0b0c0d0e0f".to_owned(),
        pid,
        spec_digest: "ab".repeat(32),
        state,
        workload: "1a2b3c4d".to_owned(),
    }
}

/// An intent reads back unchanged. Its file is private and holds the documented field spelling.
///
/// # Errors
///
/// Fails when the file is unreadable or differs from what the test wrote.
#[test]
fn an_intent_reads_back_as_written_and_is_private() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let written = intent(State::Starting, Some(4242));
    written.write(dir.path())?;
    ensure!(Intent::read(dir.path())? == written, "the intent changed on the way through the file");
    let mode = fs::metadata(dir.path().join(FILE))?.permissions().mode();
    ensure!(mode & 0o777 == 0o600, "the intent has mode {mode:o}");
    let text = fs::read_to_string(dir.path().join(FILE))?;
    ensure!(text.contains(r#""state":"starting""#) && text.contains(r#""pid":4242"#), "unexpected JSON: {text}");
    Ok(())
}

/// A write replaces the intent atomically.
///
/// A rewrite removes its temporary file. A temporary file left by an interrupted write doesn't affect
/// a read, and the next write replaces it.
///
/// # Errors
///
/// Fails when a rewrite keeps the old intent, a read sees the leftover, or the leftover outlasts a
/// write.
#[test]
fn a_write_replaces_the_intent_and_a_leftover_temporary_file_does_no_harm() -> Result<()> {
    let dir = tempfile::tempdir()?;
    intent(State::Intended, None).write(dir.path())?;
    intent(State::Running, Some(7)).write(dir.path())?;
    ensure!(Intent::read(dir.path())?.state == State::Running, "the rewrite didn't replace the intent");
    ensure!(!dir.path().join(TEMP).exists(), "the temporary file is still there");
    fs::write(dir.path().join(TEMP), b"{ half a write")?;
    ensure!(Intent::read(dir.path())?.state == State::Running, "the read saw the leftover");
    intent(State::Starting, Some(9)).write(dir.path())?;
    ensure!(Intent::read(dir.path())?.state == State::Starting, "the write didn't land");
    ensure!(!dir.path().join(TEMP).exists(), "the leftover outlasted the write");
    Ok(())
}

/// A missing or damaged intent is an error.
///
/// # Errors
///
/// Fails when a read of a missing or damaged file succeeds.
#[test]
fn a_missing_or_damaged_intent_is_an_error() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let missing = Intent::read(dir.path());
    ensure!(missing.is_err_and(|err| err.kind() == io::ErrorKind::NotFound), "a missing intent didn't report NotFound");
    fs::write(dir.path().join(FILE), b"not json")?;
    ensure!(Intent::read(dir.path()).is_err(), "a damaged intent read as valid");
    Ok(())
}
