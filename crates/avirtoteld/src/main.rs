//! A stub for the host daemon that collects OpenTelemetry data.

use std::process::ExitCode;

/// Prints a stub notice on stderr and exits with failure.
fn main() -> ExitCode {
    eprintln!("avirtoteld: not implemented");
    ExitCode::FAILURE
}
