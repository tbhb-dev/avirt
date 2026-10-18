//! The policy service, which stores, compiles, proves, and distributes Cedar policy snapshots.

use std::io::{Write as _, stderr};
use std::process::ExitCode;

/// Prints a stub notice on stderr and exits with failure.
fn main() -> ExitCode {
    drop(writeln!(stderr(), "avirtpolicyd: not implemented"));
    ExitCode::FAILURE
}
