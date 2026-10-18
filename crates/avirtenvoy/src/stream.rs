//! SDK-independent connection event state.
use std::time::{Duration, Instant};

use avirt_connect::Target;
use serde_json::{Value, json};

pub struct Stream {
    event: Value,
    began: Instant,
    last_emit: Instant,
    id: u64,
}

impl Stream {
    pub fn on_headers(id: u64, method: &str, authority: &str, scheme: &str, now: Instant) -> (Self, Vec<String>) {
        let target = Target::parse(authority);
        let mut stream = Self {
            event: json!({
                "event": "conn", "started": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
                "method": method, "host": target.as_ref().map_or(authority, |t| &t.host),
                "port": target.as_ref().map_or(0, |t| t.port), "decision": "deny", "rule": "undecided",
                "bytes_up": 0, "bytes_down": 0, "duration_ms": 0, "error": null
            }),
            began: now,
            last_emit: now,
            id,
        };
        let mut lines = stream.snapshot(false, now);
        if method != "CONNECT" {
            lines.extend(stream.on_refused(
                if scheme == "http" { "plain-http" } else { "unsupported-method" },
                None,
                now,
            ));
        } else if target.is_none() {
            lines.extend(stream.on_refused("bad-request", Some("bad authority"), now));
        }
        (stream, lines)
    }

    pub fn refusal(&self) -> Option<(u32, &'static str)> {
        match self.event["rule"].as_str()? {
            "plain-http" => Some((501, "plain-http")),
            "unsupported-method" => Some((501, "unsupported-method")),
            "bad-request" => Some((400, "bad-request")),
            _ => None,
        }
    }

    pub fn on_refused(&mut self, rule: &str, error: Option<&str>, now: Instant) -> Vec<String> {
        self.event["decision"] = json!(if rule == "allow-all" { "allow" } else { "deny" });
        self.event["rule"] = json!(rule);
        self.event["error"] = json!(error);
        self.snapshot(false, now)
    }

    pub fn on_allowed(&mut self, now: Instant) -> Vec<String> {
        self.event["decision"] = json!("allow");
        self.event["rule"] = json!("allow-all");
        self.snapshot(false, now)
    }

    pub fn on_bytes(&mut self, up: u64, down: u64, now: Instant) -> Vec<String> {
        self.event["bytes_up"] = json!(self.event["bytes_up"].as_u64().unwrap_or_default() + up);
        self.event["bytes_down"] = json!(self.event["bytes_down"].as_u64().unwrap_or_default() + down);
        if now.duration_since(self.last_emit) >= Duration::from_secs(1) {
            self.snapshot(false, now)
        } else {
            Vec::new()
        }
    }

    pub fn on_complete(&mut self, response_code: Option<i64>, now: Instant) -> Vec<String> {
        if self.event["rule"] == "undecided" {
            self.event["error"] = json!("aborted");
        } else if self.event["rule"] == "allow-all"
            && self.event["error"].is_null()
            && response_code.is_some_and(|c| c >= 400)
        {
            self.event["error"] = json!("dial");
        }
        self.snapshot(true, now)
    }

    fn snapshot(&mut self, complete: bool, now: Instant) -> Vec<String> {
        self.last_emit = now;
        self.event["duration_ms"] =
            json!(u64::try_from(now.duration_since(self.began).as_millis()).unwrap_or(u64::MAX));
        vec![json!({"avirt_conn": self.id, "complete": complete, "event": self.event}).to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(lines: &[String]) -> Value {
        serde_json::from_str::<Value>(lines.last().unwrap()).unwrap()["event"].clone()
    }

    fn assert_state(lines: &[String], decision: &str, rule: &str, error: Option<&str>) {
        let e = event(lines);
        assert_eq!(e["decision"], decision);
        assert_eq!(e["rule"], rule);
        assert_eq!(e["error"], json!(error));
    }

    #[test]
    fn unchecked_stream_never_allows() {
        let now = Instant::now();
        let (mut stream, mut lines) = Stream::on_headers(1, "CONNECT", "example.com:443", "", now);
        lines.extend(stream.on_complete(None, now));
        assert!(lines.iter().all(|line| serde_json::from_str::<Value>(line).unwrap()["event"]["decision"] == "deny"));
        assert_state(&lines, "deny", "undecided", Some("aborted"));
    }

    #[test]
    fn refused_stream_stays_denied() {
        let now = Instant::now();
        let (mut stream, _) = Stream::on_headers(1, "CONNECT", "example.com:443", "", now);
        stream.on_refused("private-dest", None, now);
        assert_state(&stream.on_complete(Some(403), now), "deny", "private-dest", None);
    }

    #[test]
    fn allowed_stream_maps_dial_errors() {
        let now = Instant::now();
        for (code, error) in [(200, None), (503, Some("dial"))] {
            let (mut stream, _) = Stream::on_headers(1, "CONNECT", "example.com:443", "", now);
            assert_state(&stream.on_allowed(now), "allow", "allow-all", None);
            assert_state(&stream.on_complete(Some(code), now), "allow", "allow-all", error);
        }
        let (mut stream, _) = Stream::on_headers(1, "CONNECT", "example.com:443", "", now);
        stream.on_refused("allow-all", Some("dns"), now);
        assert_state(&stream.on_complete(Some(502), now), "allow", "allow-all", Some("dns"));
    }

    #[test]
    fn progress_is_throttled_without_losing_bytes() {
        let now = Instant::now();
        let (mut stream, _) = Stream::on_headers(1, "CONNECT", "example.com:443", "", now);
        stream.on_allowed(now);
        let count: usize = (0..1000).map(|_| stream.on_bytes(10, 20, now).len()).sum();
        assert!(count <= 1);
        let later = now + Duration::from_millis(1100);
        assert_eq!(stream.on_bytes(10, 20, later).len(), 1);
        let e = event(&stream.on_complete(Some(200), later));
        assert_eq!(e["bytes_up"], 10010);
        assert_eq!(e["bytes_down"], 20020);
    }

    #[test]
    fn header_refusals() {
        let now = Instant::now();
        let (mut stream, _) = Stream::on_headers(1, "GET", "example.com:80", "http", now);
        assert_state(&stream.on_complete(Some(501), now), "deny", "plain-http", None);
        let (mut stream, _) = Stream::on_headers(1, "CONNECT", "bad", "", now);
        assert_state(&stream.on_complete(Some(400), now), "deny", "bad-request", Some("bad authority"));
    }
}
