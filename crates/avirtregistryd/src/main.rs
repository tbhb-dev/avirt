//! The workspace image registry, which wraps an OCI registry and adds avirt's index and search API for workspace images.

use std::process::ExitCode;

/// Prints a stub notice on stderr and exits with failure.
fn main() -> ExitCode {
    eprintln!("avirtregistryd: not implemented");
    ExitCode::FAILURE
}
