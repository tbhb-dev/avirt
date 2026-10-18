//! The audit logger, which delivers audit events as OCSF.

use std::io::{Write as _, stderr};
use std::process::ExitCode;

/// Prints a stub notice on stderr and exits with failure.
fn main() -> ExitCode {
    drop(writeln!(stderr(), "avirtauditd: not implemented"));
    ExitCode::FAILURE
}
