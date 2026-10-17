//! The workload's entrypoint. `AVIRT_AGENT` (set in each image) selects the agent: `claude`
//! speaks ACP v2, `codex` speaks ACP v1.

mod claude;
mod codex;

use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    match std::env::var("AVIRT_AGENT").as_deref() {
        Ok("claude") => claude::run().await,
        Ok("codex") => codex::run().await,
        other => {
            eprintln!("AVIRT_AGENT must be claude or codex, got {other:?}");
            ExitCode::from(64)
        }
    }
}
