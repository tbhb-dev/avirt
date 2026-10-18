//! The mediator's event channel to avirthostd: one NDJSON line per proxied connection, written
//! by a dedicated thread to a second connection on avirthostd's socket.

use std::io::{Result as IoResult, Write as _};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::mpsc::{Sender, channel};
use std::sync::{Mutex, OnceLock, PoisonError};
use std::thread::{JoinHandle, spawn};

/// The sending end of the writer thread's queue, set once by `connect`.
static TX: OnceLock<Sender<Option<String>>> = OnceLock::new();
/// The writer thread, joined by `close`.
static WRITER: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

/// Flushes everything queued and closes the connection.
pub(crate) fn close() {
    if let Some(tx) = TX.get() {
        tx.send(None).unwrap_or_default();
    }
    let writer = WRITER.lock().unwrap_or_else(PoisonError::into_inner).take();
    if let Some(handle) = writer {
        handle.join().unwrap_or_default();
    }
}

/// Connects, identifies the workload to avirthostd, and starts the writer thread.
///
/// # Errors
///
/// Returns the I/O error if the connection or the preamble write fails.
pub(crate) fn connect(hostd_sock: &Path, workload: &str, secret: &str) -> IoResult<()> {
    let mut conn = UnixStream::connect(hostd_sock)?;
    conn.write_all(format!("EGRESS {workload} {secret}\n").as_bytes())?;
    let (tx, rx) = channel::<Option<String>>();
    let writer = spawn(move || {
        // `None` is the close sentinel. A failed write (avirthostd is unavailable) drops later events.
        while let Ok(Some(line)) = rx.recv() {
            conn.write_all(line.as_bytes()).and_then(|()| conn.write_all(b"\n")).unwrap_or_default();
        }
    });
    TX.set(tx).unwrap_or_default();
    *WRITER.lock().unwrap_or_else(PoisonError::into_inner) = Some(writer);
    Ok(())
}

/// Queues one event line for the writer thread.
pub(crate) fn send(line: String) {
    if let Some(tx) = TX.get() {
        tx.send(Some(line)).unwrap_or_default();
    }
}
