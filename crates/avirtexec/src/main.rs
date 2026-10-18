//! A stub for the workload process that runs commands.

use std::io::{Write as _, stderr};
use std::process::ExitCode;

/// Prints a stub notice on stderr and exits with failure.
fn main() -> ExitCode {
    drop(writeln!(stderr(), "avirtexec: not implemented"));
    ExitCode::FAILURE
}
