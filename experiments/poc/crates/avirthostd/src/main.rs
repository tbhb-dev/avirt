//! The host daemon (`avirthostd`) that launches either agent in an Apple container.

extern crate alloc;

pub mod codex_auth;
pub mod profiles;

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use alloc::sync::Arc;
use anyhow::{Result, bail};
use avirt_core::api::DEFAULT_PORT;
use avirt_core::console;
use avirt_core::hostd;
use avirt_core::hostd::profile::Config;
use avirt_core::runtime::Runtime;
use avirt_core::runtime::container_cli::ContainerCli;
use avirt_core::runtime::containerization::workload::Containerization;
use avirt_core::state;
use clap::{Parser, ValueEnum};

/// How avirthostd runs workloads.
#[derive(Clone, Copy, Debug, ValueEnum)]
#[value(rename_all = "lower")]
enum RuntimeKind {
    /// Apple's `container` command.
    Container,
    /// A per-workload avirtvmd that hosts the VM through Containerization.
    Containerization,
}

/// The command-line options.
#[derive(Parser)]
#[command(about = None)]
struct Args {
    /// The directory of OCI archives that `mise run build-image` writes.
    #[arg(
        long,
        env = "AVIRT_IMAGE_ARCHIVES",
        default_value = "target/images",
        help = None,
        display_order = 5
    )]
    image_archives: PathBuf,
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
    /// How workloads run.
    #[arg(
        long,
        env = "AVIRT_RUNTIME",
        value_enum,
        default_value = "containerization",
        help = None,
        display_order = 3
    )]
    runtime: RuntimeKind,
    /// The per-workload VM host helper.
    #[arg(
        long,
        env = "AVIRT_VMD_BIN",
        help = "The per-workload VM host [default: avirtvmd next to this binary]",
        display_order = 4
    )]
    vmd_bin: Option<PathBuf>,
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
    let config = Config { port: args.port, logs_dir: args.logs_dir, mediator_bin };
    match args.runtime {
        RuntimeKind::Container => hostd::serve(profiles, |_| Ok(Arc::new(ContainerCli)), config).await,
        RuntimeKind::Containerization => {
            let vmd_bin = match args.vmd_bin {
                Some(bin) => bin,
                None => env::current_exe()?.with_file_name("avirtvmd"),
            };
            if !vmd_bin.is_file() {
                bail!("{} not found; build it with `mise run build-vmd` or set AVIRT_VMD_BIN", vmd_bin.display());
            }
            let archives = args.image_archives;
            let make = move |state: &state::Root| -> Result<Arc<dyn Runtime>> {
                Ok(Arc::new(Containerization::new(state, vmd_bin, archives)))
            };
            hostd::serve(profiles, make, config).await
        }
    }
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
