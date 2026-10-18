//! A stub for the host MCP gateway that proxies workload tool calls.

use std::io::{Write as _, stderr};
use std::process::ExitCode;

/// Prints a stub notice on stderr and exits with failure.
fn main() -> ExitCode {
    drop(writeln!(stderr(), "avirtmcpgwd: not implemented"));
    ExitCode::FAILURE
}
