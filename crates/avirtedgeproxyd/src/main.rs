//! The edge proxy evaluates host policy and resolves external names. It checks private addresses before opening upstream connections.

use std::io::{Write as _, stderr};
use std::process::ExitCode;

/// Prints a stub notice on stderr and exits with failure.
fn main() -> ExitCode {
    drop(writeln!(stderr(), "avirtedgeproxyd: not implemented"));
    ExitCode::FAILURE
}
