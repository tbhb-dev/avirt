//! The workload services daemon provides a file server and snapshots, plus the guest MCP tool surface.

use std::process::ExitCode;

/// Prints a stub notice on stderr and exits with failure.
fn main() -> ExitCode {
    eprintln!("avirtguestd: not implemented");
    ExitCode::FAILURE
}
