//! The workload process that runs an agent through its native harness.

use std::io::{Write as _, stderr};
use std::process::ExitCode;

/// Prints a stub notice on stderr and exits with failure.
fn main() -> ExitCode {
    drop(writeln!(stderr(), "avirtagent: not implemented"));
    ExitCode::FAILURE
}
