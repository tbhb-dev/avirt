//! One host controller that launches either agent in an Apple container.

mod codex_auth;
mod profiles;

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;

use agentvmm_core::api::DEFAULT_PORT;
use agentvmm_core::controller::{self, Config};
use anyhow::{Result, bail};
use clap::Parser;

#[derive(Parser)]
struct Args {
    #[arg(long, env = "PORT", default_value_t = DEFAULT_PORT)]
    port: u16,
    #[arg(long, env = "LOGS_DIR", default_value = "logs")]
    logs_dir: PathBuf,
    /// The per-actor supervisor [default: agentvmm-supervisor next to this binary]
    #[arg(long, env = "AGENTVMM_SUPERVISOR_BIN")]
    supervisor_bin: Option<PathBuf>,
}

async fn serve(args: Args) -> Result<()> {
    let profiles: HashMap<_, _> = profiles::all()?;
    let supervisor_bin = match args.supervisor_bin {
        Some(bin) => bin,
        None => std::env::current_exe()?.with_file_name("agentvmm-supervisor"),
    };
    if !supervisor_bin.is_file() {
        bail!(
            "{} not found; build it with `cargo build -p agentvmm-supervisor` or set AGENTVMM_SUPERVISOR_BIN",
            supervisor_bin.display()
        );
    }
    controller::serve(profiles, Config { port: args.port, logs_dir: args.logs_dir, supervisor_bin }).await
}

#[tokio::main]
async fn main() -> ExitCode {
    match serve(Args::parse()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err:#}");
            ExitCode::FAILURE
        }
    }
}
