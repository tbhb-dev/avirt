//! The host daemon (`avirthostd`) that launches either agent in an Apple container.

extern crate alloc;

pub mod codex_auth;
pub mod profiles;

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Result, bail};
use avirt_core::api::DEFAULT_PORT;
use avirt_core::console;
use avirt_core::hostd;
use avirt_core::hostd::profile::Config;
use clap::Parser;

/// The command-line options.
#[derive(Parser)]
#[command(about = None)]
struct Args {
    /// The directory for the workload logs.
    #[arg(long, env = "LOGS_DIR", default_value = "logs", help = None, display_order = 1)]
    logs_dir: PathBuf,
    /// The path to the per-workload mediator binary.
    #[arg(
        long,
        env = "AVIRT_MEDIATOR_BIN",
        help = "The per-workload mediator [default: avirtmediatord next to this binary]",
        display_order = 2
    )]
    mediator_bin: Option<PathBuf>,
    /// The port avirthostd listens on.
    #[arg(long, env = "PORT", default_value_t = DEFAULT_PORT, help = None, display_order = 0)]
    port: u16,
}

/// Builds the agent profiles, finds the mediator binary and runs the daemon until it stops.
///
/// # Errors
///
/// Returns an error if the profiles fail to build, the mediator binary is missing, or the
/// daemon fails.
async fn serve(args: Args) -> Result<()> {
    let profiles = profiles::all()?;
    let mediator_bin = match args.mediator_bin {
        Some(bin) => bin,
        None => env::current_exe()?.with_file_name("avirtmediatord"),
    };
    if !mediator_bin.is_file() {
        bail!(
            "{} not found; build it with `cargo build -p avirtmediatord` or set AVIRT_MEDIATOR_BIN",
            mediator_bin.display()
        );
    }
    hostd::serve(profiles, Config { port: args.port, logs_dir: args.logs_dir, mediator_bin }).await
}

/// Parses the options and runs the daemon. A failure goes to stderr.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::main]
async fn main() -> ExitCode {
    match serve(Args::parse()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            console::err_line(format_args!("{err:#}"));
            ExitCode::FAILURE
        }
    }
}
