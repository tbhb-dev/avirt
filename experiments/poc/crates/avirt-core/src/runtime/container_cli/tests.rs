//! Unit tests for the `container run` command line.

use super::command;
use crate::egress::WorkloadDir;
use crate::hostd::profile::Share;
use crate::runtime::WorkloadSpec;
use anyhow::{Context as _, Result, ensure};
use std::path::PathBuf;

/// The command line holds variable names and never their values, and the image comes last.
///
/// # Errors
///
/// Fails when the fixture fails to build or the command line breaks an expectation.
#[test]
fn no_value_reaches_the_command_line() -> Result<()> {
    let base = tempfile::tempdir()?;
    let spec = WorkloadSpec {
        dir: WorkloadDir::create(base.path(), "00000001")?,
        env: vec![("SECRET".to_owned(), "hunter2".to_owned())],
        id: "00000001".to_owned(),
        image: "image:dev".to_owned(),
        logs_dir: base.path().to_path_buf(),
        shares: vec![Share { guest: "/guest/dir", host: PathBuf::from("/host/dir") }],
    };
    let cmd = command(&spec)?;
    let args: Vec<String> = cmd.as_std().get_args().map(|arg| arg.to_string_lossy().into_owned()).collect();
    ensure!(args.iter().all(|arg| !arg.contains("hunter2")), "a value is on the command line: {args:?}");
    ensure!(args.windows(2).any(|pair| pair == ["-e", "SECRET"]), "no -e SECRET in {args:?}");
    ensure!(args.windows(2).any(|pair| pair == ["-v", "/host/dir:/guest/dir"]), "no share in {args:?}");
    ensure!(args.last().map(String::as_str) == Some("image:dev"), "the image isn't last in {args:?}");
    let secret = cmd.as_std().get_envs().find(|(name, _)| *name == "SECRET").and_then(|(_, value)| value);
    let value = secret.context("SECRET isn't in the command's environment")?;
    ensure!(value.to_string_lossy() == "hunter2", "SECRET has the wrong value in the environment");
    Ok(())
}
