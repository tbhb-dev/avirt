//! A stub for the workload process that runs commands.

use std::process::ExitCode;

/// Prints a stub notice on stderr and exits with failure.
fn main() -> ExitCode {
    eprintln!("avirtexec: not implemented");
    ExitCode::FAILURE
}
