//! The workload's entrypoint. `AVIRT_AGENT` (set in each image) selects the agent: `claude`
//! speaks ACP v2, `codex` speaks ACP v1.

mod claude;
mod codex;

use std::env;
use std::process::ExitCode;

/// Runs the agent that `AVIRT_AGENT` names.
///
/// # Panics
///
/// Panics if the async runtime can't start.
#[tokio::main]
async fn main() -> ExitCode {
    let agent = env::var("AVIRT_AGENT").unwrap_or_default();
    match agent.as_str() {
        "claude" => claude::run().await,
        "codex" => codex::run().await,
        other => {
            eprintln!("AVIRT_AGENT must be claude or codex, got \"{other}\"");
            ExitCode::from(64)
        }
    }
}
