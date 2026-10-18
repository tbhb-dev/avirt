//! The builder, which watches VCS and builds repository OCI artifacts for fast workload startup.

use std::io::{Write as _, stderr};
use std::process::ExitCode;

/// Prints a stub notice on stderr and exits with failure.
fn main() -> ExitCode {
    drop(writeln!(stderr(), "avirtbuilderd: not implemented"));
    ExitCode::FAILURE
}
