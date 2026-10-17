//! The host daemon (`avirthostd`) that launches either agent in an Apple container.

mod codex_auth;
mod profiles;

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Result, bail};
use avirt_core::api::DEFAULT_PORT;
use avirt_core::hostd::{self, Config};
use clap::Parser;

#[derive(Parser)]
struct Args {
    #[arg(long, env = "PORT", default_value_t = DEFAULT_PORT)]
    port: u16,
    #[arg(long, env = "LOGS_DIR", default_value = "logs")]
    logs_dir: PathBuf,
    /// The per-actor mediator [default: avirtmediatord next to this binary]
    #[arg(long, env = "AVIRT_MEDIATOR_BIN")]
    mediator_bin: Option<PathBuf>,
}

async fn serve(args: Args) -> Result<()> {
    let profiles: HashMap<_, _> = profiles::all()?;
    let mediator_bin = match args.mediator_bin {
        Some(bin) => bin,
        None => std::env::current_exe()?.with_file_name("avirtmediatord"),
    };
    if !mediator_bin.is_file() {
        bail!(
            "{} not found; build it with `cargo build -p avirtmediatord` or set AVIRT_MEDIATOR_BIN",
            mediator_bin.display()
        );
    }
    hostd::serve(profiles, Config { port: args.port, logs_dir: args.logs_dir, mediator_bin }).await
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
