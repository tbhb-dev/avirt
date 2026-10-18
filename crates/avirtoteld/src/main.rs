//! A stub for the host daemon that collects OpenTelemetry data.

use std::io::{Write as _, stderr};
use std::process::ExitCode;

/// Prints a stub notice on stderr and exits with failure.
fn main() -> ExitCode {
    drop(writeln!(stderr(), "avirtoteld: not implemented"));
    ExitCode::FAILURE
}
