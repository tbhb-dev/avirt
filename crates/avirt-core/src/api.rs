//! `avirthostd`'s local control API, shared by `avirthostd` and the command-line interface.
//!
//! The daemon serves these types and the command-line tool sends them, so a field renamed on one side breaks the other at compile time. Each endpoint has a path constant, and the request and reply types serialize to plain JSON objects.
//!
//! Today the API has a health probe and a launch request. A launch request specifies an agent, an optional workspace directory, and a prompt. Every field defaults to empty, which lets avirthostd's own validation see a malformed body instead of the JSON layer rejecting it, and the daemon can answer with an error the operator reads. The reply gives the new workload's id and the log file that records its events. The port constant is the one place that fixes where a local daemon listens by default.

#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The port avirthostd binds on loopback unless told otherwise.
///
/// The command-line tool and the daemon share this constant, so the command-line tool without a port flag finds `avirthostd` started without one.
pub const DEFAULT_PORT: u16 = 8787;
/// A `POST` to this path gets an answer whenever avirthostd is up.
pub const HEALTH_PATH: &str = "/health";
/// Workloads start through this `POST` path.
pub const LAUNCH_PATH: &str = "/launch";

/// Body of `POST /launch`. A missing field arrives empty and fails avirthostd's validation.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LaunchRequest {
    /// Which agent to run, such as `claude` or `codex`.
    pub agent: String,
    /// Absolute host directory to mount at `/workspace`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// What the agent receives as its first prompt, verbatim.
    pub prompt: String,
}

/// The reply to a successful `POST /launch`.
///
/// The client uses it to find the log file and to refer to the workload afterwards.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchResponse {
    /// The path of the workload's NDJSON log file.
    pub log_path: String,
    /// The new workload's identifier.
    pub workload_id: String,
}

/// The control API's base URL for avirthostd on the local machine.
#[must_use]
pub fn default_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

/// Whether a log line is the `container_exit` meta event, which is always the last event of a
/// workload. Parses each line as JSON without pattern-matching raw text.
#[must_use]
pub fn is_terminal_event(line: &str) -> bool {
    serde_json::from_str::<Value>(line).is_ok_and(|event| {
        event.get("dir").is_some_and(|dir| dir == "meta")
            && event.get("msg").and_then(|msg| msg.get("event")).is_some_and(|name| name == "container_exit")
    })
}
