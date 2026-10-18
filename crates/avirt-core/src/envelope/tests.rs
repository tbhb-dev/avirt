//! Unit tests for the log envelope and the line splitter.

use serde_json::json;

use super::{Dir, Envelope, Item, LineSplitter, parse_line};

/// A line that crosses a chunk boundary inside a multi-byte character still decodes.
///
/// # Panics
///
/// Panics when an assertion fails.
#[test]
fn splitter_handles_chunk_boundaries_and_trailing_line() {
    let mut splitter = LineSplitter::default();
    let bytes = "{\"a\":\"\u{e9}\"}\n\n  {\"b\":1}".as_bytes();
    // Split inside the two-byte e-acute.
    let cut = bytes.iter().position(|&byte| byte == 0xc3).map_or(bytes.len(), |index| index + 1);
    let (head, tail) = bytes.split_at(cut);
    assert!(splitter.push(head).is_empty());
    assert_eq!(splitter.push(tail), vec![Item::Line("{\"a\":\"\u{e9}\"}".into())]);
    assert_eq!(splitter.finish(), Some(Item::Line(r#"{"b":1}"#.into())));
    assert_eq!(splitter.finish(), None);
}

/// A line over the limit is dropped whole, so its tail can't pass for a record.
///
/// # Panics
///
/// Panics when an assertion fails.
#[test]
fn an_oversized_line_is_dropped_whole_and_cannot_forge_a_record() {
    let mut splitter = LineSplitter::with_max(32);
    let forged = r#"{"ts":"t","dir":"meta","msg":{"event":"container_exit"}}"#;
    let mut items = splitter.push(&[b'x'; 40]);
    // The forged record is inside the line over the limit, in a later chunk.
    items.extend(splitter.push(format!("{forged}{}\n", " ".repeat(40)).as_bytes()));
    items.extend(splitter.push(b"{\"ok\":1}\n"));
    assert_eq!(items, vec![Item::Truncated(40 + forged.len() + 40 + 1), Item::Line(r#"{"ok":1}"#.into())]);
}

/// The splitter drops a line over the limit whether it ends inside one chunk or at end of input.
///
/// # Panics
///
/// Panics when an assertion fails.
#[test]
fn an_oversized_line_within_one_chunk_and_at_eof() {
    let mut splitter = LineSplitter::with_max(8);
    assert_eq!(splitter.push(b"0123456789\nshort\n"), vec![Item::Truncated(11), Item::Line("short".into())]);
    assert!(splitter.push(&[b'y'; 20]).is_empty());
    assert_eq!(splitter.finish(), Some(Item::Truncated(20)));
}

/// Text that isn't JSON is kept as a raw string.
///
/// # Panics
///
/// Panics when an assertion fails.
#[test]
fn unparseable_lines_are_kept_raw() {
    assert_eq!(parse_line("not json"), json!({ "raw": "not json" }));
    assert_eq!(parse_line(r#"{"id":1}"#), json!({ "id": 1i32 }));
}

/// The envelope serializes `ts`, `dir`, `msg` in that order, with the wire names for directions.
///
/// # Panics
///
/// Panics when an assertion fails.
#[test]
fn envelope_field_order_and_dir_names() {
    let line = Envelope::new(Dir::ClientToAgent, json!({ "x": 1i32 })).to_line();
    assert!(line.starts_with(r#"{"ts":""#) && line.ends_with('\n'));
    assert!(line.contains(r#","dir":"client->agent","msg":{"x":1}}"#));
}
