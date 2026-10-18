//! The guest networking daemon provides the link endpoint, egress, ingress, stub DNS resolver, and guest control API.

use std::process::ExitCode;

/// Prints a stub notice on stderr and exits with failure.
fn main() -> ExitCode {
    eprintln!("avirtnetd: not implemented");
    ExitCode::FAILURE
}
