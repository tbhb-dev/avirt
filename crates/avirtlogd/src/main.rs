//! The log collector, which gathers workload stdout and stderr, transforms it and sends it to avirtoteld as OpenTelemetry Logs.

use std::process::ExitCode;

/// Prints a stub notice on stderr and exits with failure.
fn main() -> ExitCode {
    eprintln!("avirtlogd: not implemented");
    ExitCode::FAILURE
}
