//! The workload process that runs an agent through its native harness.

use std::process::ExitCode;

/// Prints a stub notice on stderr and exits with failure.
fn main() -> ExitCode {
    eprintln!("avirtagent: not implemented");
    ExitCode::FAILURE
}
