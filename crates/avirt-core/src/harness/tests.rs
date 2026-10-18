//! Unit tests for the harness module.

use super::pick_option;

/// Approval prefers `allow_once`, then `allow_always`, then the first option.
///
/// # Panics
///
/// Panics when an assertion fails.
#[test]
fn approval_prefers_allow_once_then_always_then_first() {
    let opts = [("a", "reject_once"), ("b", "allow_always"), ("c", "allow_once")];
    assert_eq!(pick_option(&opts, |option| option.1).map(|option| option.0), Some("c"));
    assert_eq!(pick_option(opts.get(..2).unwrap_or_default(), |option| option.1).map(|option| option.0), Some("b"));
    assert_eq!(pick_option(opts.get(..1).unwrap_or_default(), |option| option.1).map(|option| option.0), Some("a"));
    let empty: [(&str, &str); 0] = [];
    assert!(pick_option(&empty, |option| option.1).is_none());
}
