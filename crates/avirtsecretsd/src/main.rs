//! The secrets broker fetches secrets from providers and serves the credential broker API.

use std::io::{Write as _, stderr};
use std::process::ExitCode;

/// Prints a stub notice on stderr and exits with failure.
fn main() -> ExitCode {
    drop(writeln!(stderr(), "avirtsecretsd: not implemented"));
    ExitCode::FAILURE
}
