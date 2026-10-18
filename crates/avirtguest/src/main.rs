//! The guest CLI is in the image and connects to avirtapid through the mediator.

use std::process::ExitCode;

/// Prints a stub notice on stderr and exits with failure.
fn main() -> ExitCode {
    eprintln!("avirtguest: not implemented");
    ExitCode::FAILURE
}
