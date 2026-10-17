//! The supervisor's event channel to the daemon: one NDJSON line per proxied connection, written
//! by a dedicated thread to a second connection on the daemon's socket.

use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::mpsc::{Sender, channel};
use std::sync::{Mutex, OnceLock};
use std::thread::JoinHandle;

static TX: OnceLock<Sender<Option<String>>> = OnceLock::new();
static WRITER: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

/// Connects, identifies the session to the daemon and starts the writer thread.
pub fn connect(daemon_sock: &Path, session: &str, secret: &str) -> std::io::Result<()> {
    let mut conn = UnixStream::connect(daemon_sock)?;
    conn.write_all(format!("EGRESS {session} {secret}\n").as_bytes())?;
    let (tx, rx) = channel::<Option<String>>();
    let writer = std::thread::spawn(move || {
        // `None` is the close sentinel; a failed write (the daemon is gone) drops later events.
        while let Ok(Some(line)) = rx.recv() {
            let _ = conn.write_all(line.as_bytes()).and_then(|()| conn.write_all(b"\n"));
        }
    });
    let _ = TX.set(tx);
    *WRITER.lock().expect("writer lock") = Some(writer);
    Ok(())
}

pub fn send(line: String) {
    if let Some(tx) = TX.get() {
        let _ = tx.send(Some(line));
    }
}

/// Flushes everything queued and closes the connection.
pub fn close() {
    if let Some(tx) = TX.get() {
        let _ = tx.send(None);
    }
    if let Some(writer) = WRITER.lock().expect("writer lock").take() {
        let _ = writer.join();
    }
}
