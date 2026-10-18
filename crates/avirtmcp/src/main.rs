//! The stdio MCP relay to avirtmcpgwd, launched on demand by the agent.

use std::process::ExitCode;

/// Prints a stub notice on stderr and exits with failure.
fn main() -> ExitCode {
    eprintln!("avirtmcp: not implemented");
    ExitCode::FAILURE
}
