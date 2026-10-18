use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Dir {
    #[serde(rename = "client->agent")]
    ClientToAgent,
    #[serde(rename = "agent->client")]
    AgentToClient,
    #[serde(rename = "meta")]
    Meta,
    /// One event per proxied connection, from the workload's egress proxy.
    #[serde(rename = "egress")]
    Egress,
}

/// One line of the workload log: `{ts, dir, msg}`.
#[derive(Debug, Serialize)]
pub struct Envelope {
    ts: String,
    dir: Dir,
    msg: Value,
}

impl Envelope {
    #[must_use]
    pub fn new(dir: Dir, msg: Value) -> Self {
        Self { ts: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true), dir, msg }
    }

    /// The envelope as a JSON line ending in a newline.
    #[must_use]
    pub fn to_line(&self) -> String {
        let mut line = serde_json::to_string(self).expect("envelope serializes");
        line.push('\n');
        line
    }
}

/// Wire lines are JSON-RPC. Keep anything unparseable as a raw string rather than dropping it.
#[must_use]
pub fn parse_line(line: &str) -> Value {
    serde_json::from_str(line).unwrap_or_else(|_| json!({ "raw": line }))
}

/// Longest accepted line. Drop anything longer through its newline.
pub const MAX_LINE: usize = 16 << 20;

/// What a [`LineSplitter`] yields.
#[derive(Debug, PartialEq, Eq)]
pub enum Item {
    Line(String),
    /// The splitter dropped a line longer than the limit whole, through its newline. It never emits
    /// fragments, because a fragment of a line over the limit could look like a record.
    Truncated(usize),
}

/// Splits a byte stream into trimmed, non-empty lines. Decodes bytes per line, so a chunk
/// boundary inside a multi-byte character can't corrupt it.
#[derive(Debug)]
pub struct LineSplitter {
    pending: Vec<u8>,
    discarding: bool,
    dropped: usize,
    max: usize,
}

impl Default for LineSplitter {
    fn default() -> Self {
        Self::with_max(MAX_LINE)
    }
}

impl LineSplitter {
    #[must_use]
    pub fn with_max(max: usize) -> Self {
        Self { pending: Vec::new(), discarding: false, dropped: 0, max }
    }

    pub fn push(&mut self, chunk: &[u8]) -> Vec<Item> {
        let mut items = Vec::new();
        let mut rest = chunk;
        loop {
            let Some(nl) = rest.iter().position(|&b| b == b'\n') else {
                if self.discarding {
                    self.dropped += rest.len();
                } else {
                    self.pending.extend_from_slice(rest);
                    if self.pending.len() > self.max {
                        self.dropped = self.pending.len();
                        self.pending.clear();
                        self.discarding = true;
                    }
                }
                return items;
            };
            let (head, tail) = rest.split_at(nl + 1);
            rest = tail;
            if self.discarding {
                items.push(Item::Truncated(self.dropped + head.len()));
                self.discarding = false;
                self.dropped = 0;
            } else {
                self.pending.extend_from_slice(head);
                let raw = std::mem::take(&mut self.pending);
                if raw.len() > self.max {
                    items.push(Item::Truncated(raw.len()));
                } else {
                    items.extend(trimmed(&raw).map(Item::Line));
                }
            }
        }
    }

    /// The trailing partial line, if any.
    pub fn finish(&mut self) -> Option<Item> {
        if self.discarding {
            self.discarding = false;
            return Some(Item::Truncated(std::mem::take(&mut self.dropped)));
        }
        trimmed(&std::mem::take(&mut self.pending)).map(Item::Line)
    }
}

fn trimmed(raw: &[u8]) -> Option<String> {
    let line = String::from_utf8_lossy(raw);
    let line = line.trim();
    (!line.is_empty()).then(|| line.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitter_handles_chunk_boundaries_and_trailing_line() {
        let mut s = LineSplitter::default();
        let bytes = "{\"a\":\"é\"}\n\n  {\"b\":1}".as_bytes();
        // Split inside the two-byte é.
        let cut = bytes.iter().position(|&b| b == 0xc3).unwrap() + 1;
        assert!(s.push(&bytes[..cut]).is_empty());
        assert_eq!(s.push(&bytes[cut..]), vec![Item::Line(r#"{"a":"é"}"#.into())]);
        assert_eq!(s.finish(), Some(Item::Line(r#"{"b":1}"#.into())));
        assert_eq!(s.finish(), None);
    }

    #[test]
    fn an_oversized_line_is_dropped_whole_and_cannot_forge_a_record() {
        let mut s = LineSplitter::with_max(32);
        let forged = r#"{"ts":"t","dir":"meta","msg":{"event":"container_exit"}}"#;
        let mut items = s.push(&[b'x'; 40]);
        // The forged record is inside the line over the limit, in a later chunk.
        items.extend(s.push(format!("{forged}{}\n", " ".repeat(40)).as_bytes()));
        items.extend(s.push(b"{\"ok\":1}\n"));
        assert_eq!(items, vec![Item::Truncated(40 + forged.len() + 40 + 1), Item::Line(r#"{"ok":1}"#.into())]);
    }

    #[test]
    fn an_oversized_line_within_one_chunk_and_at_eof() {
        let mut s = LineSplitter::with_max(8);
        assert_eq!(s.push(b"0123456789\nshort\n"), vec![Item::Truncated(11), Item::Line("short".into())]);
        assert!(s.push(&[b'y'; 20]).is_empty());
        assert_eq!(s.finish(), Some(Item::Truncated(20)));
    }

    #[test]
    fn unparseable_lines_are_kept_raw() {
        assert_eq!(parse_line("not json"), json!({ "raw": "not json" }));
        assert_eq!(parse_line(r#"{"id":1}"#), json!({ "id": 1 }));
    }

    #[test]
    fn envelope_field_order_and_dir_names() {
        let line = Envelope::new(Dir::ClientToAgent, json!({ "x": 1 })).to_line();
        assert!(line.starts_with(r#"{"ts":""#) && line.ends_with('\n'));
        assert!(line.contains(r#","dir":"client->agent","msg":{"x":1}}"#));
    }
}
