//! Unit tests for the launch environment.

use super::profile::EnvVar;
use super::{LaunchArgs, launch_env};
use anyhow::{Result, ensure};
use std::env;
use std::path::Path;

/// The value of `name` in `vars`, if it's set.
fn value<'vars>(vars: &'vars [(String, String)], name: &str) -> Option<&'vars str> {
    vars.iter().find(|(var, _)| var == name).map(|(_, text)| text.as_str())
}

/// Inherited and fixed variables resolve, and avirthostd's own follow them.
///
/// # Errors
///
/// Fails when a variable is missing or has the wrong value.
#[test]
fn launch_env_resolves_profile_variables_and_adds_the_avirt_ones() -> Result<()> {
    let profile = [
        EnvVar::Inherit("PATH"),
        EnvVar::Inherit("AVIRT_SURELY_UNSET_VARIABLE"),
        EnvVar::Set("CODEX_HOME", "/codex-home".to_owned()),
    ];
    let args = LaunchArgs { agent: "claude", cwd: Some(Path::new("/work")), prompt: "hello" };
    let vars = launch_env(&profile, &args, "0000000a", "tok")?;
    ensure!(value(&vars, "PATH") == env::var("PATH").ok().as_deref(), "PATH wasn't inherited");
    ensure!(value(&vars, "AVIRT_SURELY_UNSET_VARIABLE").is_none(), "an unset variable resolved");
    ensure!(value(&vars, "CODEX_HOME") == Some("/codex-home"), "the fixed variable is missing");
    ensure!(value(&vars, "AVIRT_WORKLOAD_ID") == Some("0000000a"), "the workload id is missing");
    ensure!(value(&vars, "AVIRT_CONNECTION_TOKEN") == Some("tok"), "the token is missing");
    ensure!(value(&vars, "AVIRT_PROMPT") == Some("hello"), "the prompt is missing");
    ensure!(value(&vars, "AVIRT_CWD") == Some("/workspace"), "the working directory is missing");
    let bare = LaunchArgs { agent: "claude", cwd: None, prompt: "hello" };
    ensure!(value(&launch_env(&[], &bare, "0000000a", "tok")?, "AVIRT_CWD") == Some(""), "AVIRT_CWD should be empty");
    Ok(())
}
