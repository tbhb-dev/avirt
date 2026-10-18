//! The secrets broker fetches secrets from providers and serves the credential broker API.

use std::process::ExitCode;

/// Prints a stub notice on stderr and exits with failure.
fn main() -> ExitCode {
    eprintln!("avirtsecretsd: not implemented");
    ExitCode::FAILURE
}
