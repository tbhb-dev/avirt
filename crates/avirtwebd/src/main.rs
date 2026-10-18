//! The backend for the avirt web UI.

use std::io::{Write as _, stderr};
use std::process::ExitCode;

/// Prints a stub notice on stderr and exits with failure.
fn main() -> ExitCode {
    drop(writeln!(stderr(), "avirtwebd: not implemented"));
    ExitCode::FAILURE
}
