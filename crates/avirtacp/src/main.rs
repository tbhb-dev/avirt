//! The workload process that runs an agent through the Agent Client Protocol.

use std::io::{Write as _, stderr};
use std::process::ExitCode;

/// Prints a stub notice on stderr and exits with failure.
fn main() -> ExitCode {
    drop(writeln!(stderr(), "avirtacp: not implemented"));
    ExitCode::FAILURE
}
