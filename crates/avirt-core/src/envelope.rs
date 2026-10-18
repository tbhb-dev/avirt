//! The NDJSON log envelope and the splitter that turns an agent's byte stream into lines.

#[cfg(test)]
mod tests;

use core::mem;

use chrono::{SecondsFormat, Utc};
use serde_json::{Value, json};

/// Longest accepted line. Drop anything longer through its newline.
pub const MAX_LINE: usize = 16 << 20;

/// Which way a log line traveled, or what produced it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    /// A line from the agent to the client.
    AgentToClient,
    /// A line from the client to the agent.
    ClientToAgent,
    /// One event per proxied connection, from the workload's egress proxy.
    Egress,
    /// A note from avirt itself rather than from the agent.
    Meta,
}

/// One line of the workload log: `{ts, dir, msg}`.
#[derive(Debug)]
pub struct Envelope {
    /// Which way the line traveled.
    dir: Dir,
    /// The line's JSON content.
    msg: Value,
    /// When avirthostd or avirtinitd stamped the line, as RFC 3339 with milliseconds.
    ts: String,
}

/// What a [`LineSplitter`] yields.
#[derive(Debug, PartialEq, Eq)]
pub enum Item {
    /// A trimmed, non-empty line.
    Line(String),
    /// The splitter dropped a line longer than the limit whole, through its newline. It never emits
    /// fragments, because a fragment of a line over the limit could look like a record.
    Truncated(usize),
}

/// Splits a byte stream into trimmed, non-empty lines.
///
/// Decodes bytes per line, so a chunk boundary inside a multi-byte character can't corrupt it.
#[derive(Debug)]
pub struct LineSplitter {
    /// Whether the current line already went over the limit.
    discarding: bool,
    /// Bytes dropped so far from the line that went over the limit.
    dropped: usize,
    /// The longest line to keep.
    max: usize,
    /// Bytes of the current line seen so far.
    pending: Vec<u8>,
}

impl Dir {
    /// The name that the log's `dir` key holds.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AgentToClient => "agent->client",
            Self::ClientToAgent => "client->agent",
            Self::Egress => "egress",
            Self::Meta => "meta",
        }
    }
}

impl Envelope {
    /// Stamps a message with the current time and its direction.
    #[must_use]
    pub fn new(dir: Dir, msg: Value) -> Self {
        Self { dir, msg, ts: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true) }
    }

    /// The envelope as a JSON line ending in a newline, with the keys in the order `ts`, `dir`, `msg`.
    #[must_use]
    pub fn to_line(&self) -> String {
        let mut line = json!({ "ts": self.ts, "dir": self.dir.as_str(), "msg": self.msg }).to_string();
        line.push('\n');
        line
    }
}

impl Default for LineSplitter {
    fn default() -> Self {
        Self::with_max(MAX_LINE)
    }
}

impl LineSplitter {
    /// Finishes the current line with its final chunk, newline included.
    fn complete_line(&mut self, head: &[u8]) -> Option<Item> {
        if self.discarding {
            let total = self.dropped.saturating_add(head.len());
            self.discarding = false;
            self.dropped = 0;
            return Some(Item::Truncated(total));
        }
        self.pending.extend_from_slice(head);
        let raw = mem::take(&mut self.pending);
        if raw.len() > self.max {
            return Some(Item::Truncated(raw.len()));
        }
        trimmed(&raw).map(Item::Line)
    }

    /// Adds a chunk with no newline to the current line, or starts discarding it if it's too long.
    fn extend_partial(&mut self, rest: &[u8]) {
        if self.discarding {
            self.dropped = self.dropped.saturating_add(rest.len());
            return;
        }
        self.pending.extend_from_slice(rest);
        if self.pending.len() > self.max {
            self.dropped = self.pending.len();
            self.pending.clear();
            self.discarding = true;
        }
    }

    /// The trailing partial line, if any.
    pub fn finish(&mut self) -> Option<Item> {
        if self.discarding {
            self.discarding = false;
            return Some(Item::Truncated(mem::take(&mut self.dropped)));
        }
        trimmed(&mem::take(&mut self.pending)).map(Item::Line)
    }

    /// Feeds one chunk of bytes and returns the lines it completed.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<Item> {
        let mut items = Vec::new();
        let mut rest = chunk;
        while let Some(newline) = rest.iter().position(|&byte| byte == b'\n') {
            let (head, tail) = rest.split_at(newline.saturating_add(1));
            rest = tail;
            items.extend(self.complete_line(head));
        }
        self.extend_partial(rest);
        items
    }

    /// A splitter that keeps lines up to `max` bytes.
    #[must_use]
    pub const fn with_max(max: usize) -> Self {
        Self { dropped: 0, discarding: false, max, pending: Vec::new() }
    }
}

/// Wire lines are JSON-RPC. Keep anything unparseable as a raw string rather than dropping it.
#[must_use]
pub fn parse_line(line: &str) -> Value {
    serde_json::from_str(line).unwrap_or_else(|_| json!({ "raw": line }))
}

/// The line as trimmed text, or `None` if nothing remains.
fn trimmed(raw: &[u8]) -> Option<String> {
    let decoded = String::from_utf8_lossy(raw);
    let line = decoded.trim();
    (!line.is_empty()).then(|| line.to_owned())
}
