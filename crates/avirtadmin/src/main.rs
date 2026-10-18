//! The local admin CLI.

use std::process::ExitCode;

/// Prints a stub notice on stderr and exits with failure.
fn main() -> ExitCode {
    eprintln!("avirtadmin: not implemented");
    ExitCode::FAILURE
}
