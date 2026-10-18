//! The workload services daemon provides a file server and snapshots, plus the guest MCP tool surface.

use std::io::{Write as _, stderr};
use std::process::ExitCode;

/// Prints a stub notice on stderr and exits with failure.
fn main() -> ExitCode {
    drop(writeln!(stderr(), "avirtguestd: not implemented"));
    ExitCode::FAILURE
}
