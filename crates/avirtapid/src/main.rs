//! The API server: the gRPC and REST API in front of the local avirthostd and peers' API servers, reached through the edge proxy.

use std::io::{Write as _, stderr};
use std::process::ExitCode;

/// Prints a stub notice on stderr and exits with failure.
fn main() -> ExitCode {
    drop(writeln!(stderr(), "avirtapid: not implemented"));
    ExitCode::FAILURE
}
