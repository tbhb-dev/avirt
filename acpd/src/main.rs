//! One host daemon that launches either agent in an Apple container.

mod cli;
mod codex_auth;
mod profiles;

use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    cli::main().await
}
