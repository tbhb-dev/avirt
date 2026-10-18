//! Unit tests for the launch request body.

use avirt_core::api::LaunchRequest;

/// The request body omits an unset `cwd`.
///
/// # Panics
///
/// Panics when an assertion fails.
#[test]
fn request_omits_an_unset_cwd() {
    let body = serde_json::to_value(LaunchRequest { agent: "claude".into(), prompt: "hi".into(), cwd: None }).ok();
    assert_eq!(body, Some(serde_json::json!({ "agent": "claude", "prompt": "hi" })));
}
