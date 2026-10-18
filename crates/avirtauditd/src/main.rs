//! The audit logger, which delivers audit events as OCSF.

use std::process::ExitCode;

/// Prints a stub notice on stderr and exits with failure.
fn main() -> ExitCode {
    eprintln!("avirtauditd: not implemented");
    ExitCode::FAILURE
}
