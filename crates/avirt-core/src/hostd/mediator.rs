//! The mediator connections on avirthostd's socket.

use super::{Hostd, Workload, routes::serve_workload};
use crate::envelope::{Dir, Envelope, Item, LineSplitter};
use alloc::sync::Arc;
use core::hint;
use core::sync::atomic::Ordering;
use core::time::Duration;
use serde_json::json;
use tokio::io::AsyncReadExt as _;
use tokio::net::{UnixListener, UnixStream};
use tokio::time;

/// A mediator connection's first line.
struct Preamble<'line> {
    /// The workload the connection belongs to.
    id: &'line str,
    /// `WORKLOAD` or `EGRESS`.
    kind: &'line str,
    /// The secret only this workload's mediator knows.
    secret: &'line str,
}

/// Compares two byte strings without stopping at the first difference. `black_box` keeps the
/// optimizer from turning the loop back into a comparison that exits early.
pub(crate) fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len()
        && hint::black_box(left.iter().zip(right).fold(0u8, |acc, (lhs, rhs)| acc | (lhs ^ rhs))) == 0
}

/// Accepts mediator connections on avirthostd's socket.
///
/// Each starts with one line specifying its kind and the workload it belongs to. avirthostd
/// started the mediator for that workload, so the container can't pick another. The workload's
/// HTTP/2 stream follows `WORKLOAD <id>`, with the mediator forwarding avirtinitd's bytes
/// unchanged. NDJSON proxy events follow `EGRESS <id>`.
pub(super) async fn mediator_loop(hostd: Arc<Hostd>, listener: UnixListener) -> ! {
    loop {
        let Ok((conn, _)) = listener.accept().await else {
            time::sleep(Duration::from_millis(100)).await;
            continue;
        };
        tokio::spawn(mediator_connection(Arc::clone(&hostd), conn));
    }
}

/// Serves one mediator connection after checking its preamble and secret.
async fn mediator_connection(hostd: Arc<Hostd>, mut conn: UnixStream) {
    let Some(preamble) = read_preamble(&mut conn).await else { return };
    let Some(Preamble { id, kind, secret }) = parse_preamble(&preamble) else { return };
    let Ok(workload) = hostd.lookup(id) else { return };
    // Only this workload's mediator knows the secret. A same-user process that merely knows
    // the workload id can't inject events or end the event stream early.
    if !constant_time_eq(secret.as_bytes(), workload.mediator_secret.as_bytes()) {
        return;
    }
    match kind {
        "WORKLOAD" => serve_workload(hostd, conn, id).await,
        "EGRESS" => read_egress(&workload, conn).await,
        _ => {}
    }
}

/// Splits a mediator preamble into its kind, workload id, and secret.
fn parse_preamble(line: &str) -> Option<Preamble<'_>> {
    let mut parts = line.split(' ');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(kind), Some(id), Some(secret), None) => Some(Preamble { id, kind, secret }),
        _ => None,
    }
}

/// Reads the mediator's NDJSON proxy events into the workload's log until the connection ends.
async fn read_egress(workload: &Workload, mut conn: UnixStream) {
    if workload.egress_taken.swap(true, Ordering::SeqCst) {
        return;
    }
    let mut splitter = LineSplitter::default();
    let mut buf = vec![0u8; 16 * 1024];
    while let Ok(count) = conn.read(&mut buf).await {
        let Some(chunk) = buf.get(..count).filter(|chunk| !chunk.is_empty()) else { break };
        for item in splitter.push(chunk) {
            write_egress(workload, item);
        }
    }
    if let Some(item) = splitter.finish() {
        write_egress(workload, item);
    }
    workload.egress_eof.cancel();
}

/// Reads the preamble line one byte at a time without consuming any following bytes.
async fn read_preamble(conn: &mut UnixStream) -> Option<String> {
    let read = async {
        let mut line = Vec::new();
        loop {
            let byte = conn.read_u8().await.ok()?;
            if byte == b'\n' {
                return String::from_utf8(line).ok();
            }
            line.push(byte);
            if line.len() > 128 {
                return None;
            }
        }
    };
    time::timeout(Duration::from_secs(5), read).await.ok().flatten()
}

/// Writes one proxy event from the mediator into the log as an `egress` envelope.
fn write_egress(workload: &Workload, item: Item) {
    match item {
        Item::Line(line) => {
            let event = serde_json::from_str(&line).unwrap_or_else(|_| json!({ "raw": line }));
            workload.write(&Envelope::new(Dir::Egress, event).to_line());
        }
        Item::Truncated(bytes) => workload.note_truncated(bytes),
    }
}
