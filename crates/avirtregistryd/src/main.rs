//! The workspace image registry, which wraps an OCI registry and adds avirt's index and search API for workspace images.

use std::io::{Write as _, stderr};
use std::process::ExitCode;

/// Prints a stub notice on stderr and exits with failure.
fn main() -> ExitCode {
    drop(writeln!(stderr(), "avirtregistryd: not implemented"));
    ExitCode::FAILURE
}
