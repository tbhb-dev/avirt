//! The model gateway, which injects model credentials.

use std::io::{Write as _, stderr};
use std::process::ExitCode;

/// Prints a stub notice on stderr and exits with failure.
fn main() -> ExitCode {
    drop(writeln!(stderr(), "avirtmodelgwd: not implemented"));
    ExitCode::FAILURE
}
