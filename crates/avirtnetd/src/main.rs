//! The guest networking daemon provides the link endpoint, egress, ingress, stub DNS resolver, and guest control API.

use std::io::{Write as _, stderr};
use std::process::ExitCode;

/// Prints a stub notice on stderr and exits with failure.
fn main() -> ExitCode {
    drop(writeln!(stderr(), "avirtnetd: not implemented"));
    ExitCode::FAILURE
}
