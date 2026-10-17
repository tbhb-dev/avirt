//! The controller's local control API, shared by the controller and the CLI.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Loopback port the controller listens on by default.
pub const DEFAULT_PORT: u16 = 8787;
/// `POST` path that starts an actor.
pub const LAUNCH_PATH: &str = "/launch";
/// `POST` path that answers when the controller is up.
pub const HEALTH_PATH: &str = "/health";

/// The control API's base URL for a controller on the local machine.
#[must_use]
pub fn default_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

/// Body of `POST /launch`. A missing field arrives empty and fails the controller's validation.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LaunchRequest {
    pub agent: String,
    pub prompt: String,
    /// Absolute host directory to mount at `/workspace`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
}

/// Successful reply to `POST /launch`.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchResponse {
    pub actor_id: String,
    pub log_path: String,
}

/// Whether a log line is the `container_exit` meta event, which is always the last event of an
/// actor. Each line is parsed as JSON; raw text is never pattern-matched.
#[must_use]
pub fn is_terminal_event(line: &str) -> bool {
    serde_json::from_str::<Value>(line).is_ok_and(|e| e["dir"] == "meta" && e["msg"]["event"] == "container_exit")
}

#[cfg(test)]
mod tests {
    use super::is_terminal_event;

    #[test]
    fn follow_stops_on_meta_events_only() {
        assert!(is_terminal_event(r#"{"ts":"t","dir":"meta","msg":{"event":"container_exit","code":0}}"#));
        // An error is followed by the exit event, which is the only terminal one.
        assert!(!is_terminal_event(r#"{"ts":"t","dir":"meta","msg":{"event":"container_error"}}"#));
        // The same text inside agent traffic must not stop the follow.
        assert!(!is_terminal_event(r#"{"ts":"t","dir":"agent->client","msg":{"text":"container_exit"}}"#));
        assert!(!is_terminal_event("container_exit"));
    }
}
