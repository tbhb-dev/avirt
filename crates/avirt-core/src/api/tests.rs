//! Unit tests for the control API.

use super::is_terminal_event;

/// The follow loop stops on the `container_exit` meta event and on nothing else.
///
/// # Panics
///
/// Panics when an assertion fails.
#[test]
fn follow_stops_on_meta_events_only() {
    assert!(is_terminal_event(r#"{"ts":"t","dir":"meta","msg":{"event":"container_exit","code":0}}"#));
    // The exit event follows an error and is the only terminal event.
    assert!(!is_terminal_event(r#"{"ts":"t","dir":"meta","msg":{"event":"container_error"}}"#));
    // The same text inside agent traffic must not stop the follow.
    assert!(!is_terminal_event(r#"{"ts":"t","dir":"agent->client","msg":{"text":"container_exit"}}"#));
    assert!(!is_terminal_event("container_exit"));
}
