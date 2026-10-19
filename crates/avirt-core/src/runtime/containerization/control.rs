//! The control channel to one avirtvmd.
//!
//! A dial and a handshake open it, and then requests, responses, and events share the one
//! connection. A frame is a 4-byte big-endian length and one Protocol Buffers message of at most
//! 1 MiB.

#[cfg(test)]
mod tests;

use super::lifecycle::Incarnation;
use super::proto::{
    Event, HelperBody, HelperHello, HelperMessage, HostBody, HostHello, HostMessage, Refusal, RefusalCode, Request,
    RequestOp, ResponseResult,
};
use crate::hostd::mediator::constant_time_eq;
use alloc::sync::Arc;
use anyhow::{Result, anyhow};
use bytes::Bytes;
use core::error::Error;
use core::fmt;
use core::sync::atomic::{AtomicU64, Ordering};
use core::time::Duration;
use futures_util::{SinkExt as _, StreamExt as _};
use prost::Message;
use rustix::process;
use std::collections::HashMap;
use std::io;
use std::path::Path;
use tokio::net::UnixStream;
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::time;
use tokio_util::codec::{Framed, LengthDelimitedCodec};

/// The protocol version that avirthostd speaks.
pub const PROTOCOL_VERSION: u32 = 1;

/// How long a helper has to send its hello once the connection is up.
pub const HELLO_TIMEOUT: Duration = Duration::from_secs(5);

/// The largest frame either side sends or accepts.
const MAX_FRAME: usize = 1 << 20;

/// The framed connection.
type Wire = Framed<UnixStream, LengthDelimitedCodec>;

/// What avirthostd's launch intent says the helper on a socket is.
#[derive(Clone, Copy, Debug)]
pub struct Expected<'intent> {
    /// The incarnation avirthostd gave the helper.
    pub incarnation: &'intent Incarnation,
    /// The workload the helper serves.
    pub workload: &'intent str,
}

/// Why a handshake failed.
#[derive(Debug)]
pub enum HandshakeError {
    /// The connection closed before the handshake finished.
    Closed,
    /// The socket doesn't exist yet, or nothing listens on it.
    Connect(io::Error),
    /// Reading or writing failed.
    Io(io::Error),
    /// The helper's hello didn't arrive in time.
    NoHello,
    /// The helper sent something other than a well-formed hello.
    Protocol(String),
    /// The helper presented an incarnation other than the one started.
    StaleIncarnation,
    /// The helper's version range doesn't include [`PROTOCOL_VERSION`].
    UnsupportedVersion {
        /// The newest version the helper speaks.
        max: u32,
        /// The oldest version the helper speaks.
        min: u32,
    },
    /// The process on the socket belongs to another user.
    WrongPeer,
    /// The helper serves another workload.
    WrongWorkload(String),
}

impl fmt::Display for HandshakeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Closed => f.write_str("avirtvmd closed the connection during the handshake"),
            Self::Connect(err) => write!(f, "cannot connect to avirtvmd: {err}"),
            Self::Io(err) => write!(f, "control connection failed: {err}"),
            Self::NoHello => f.write_str("avirtvmd sent no hello in time"),
            Self::Protocol(why) => write!(f, "avirtvmd broke the protocol: {why}"),
            Self::StaleIncarnation => f.write_str("avirtvmd presented an incarnation other than the one started"),
            Self::UnsupportedVersion { max, min } => {
                write!(f, "avirtvmd speaks protocol versions {min} to {max}, and avirthostd speaks {PROTOCOL_VERSION}")
            }
            Self::WrongPeer => f.write_str("the process on the socket belongs to another user"),
            Self::WrongWorkload(id) => write!(f, "avirtvmd serves workload {id}, not the one expected"),
        }
    }
}

impl Error for HandshakeError {}

/// A hello that doesn't match, with the refusal to send for it.
struct Mismatch {
    /// The refusal code.
    code: RefusalCode,
    /// The error to return.
    error: HandshakeError,
    /// The sentence for the refusal.
    note: &'static str,
}

/// One request that the pump has to send.
#[derive(Debug)]
struct Command {
    /// The request id.
    id: u64,
    /// What to ask.
    op: RequestOp,
    /// Where the answer goes.
    reply: oneshot::Sender<ResponseResult>,
}

/// What a [`Client`] shares among its clones.
#[derive(Debug)]
struct Inner {
    /// Sends requests to the pump.
    commands: mpsc::Sender<Command>,
    /// The events the pump received.
    events: Mutex<mpsc::UnboundedReceiver<Event>>,
    /// The id of the next request.
    next_id: AtomicU64,
}

/// A connection that finished the handshake. Cloning it shares the connection.
#[derive(Clone, Debug)]
pub struct Client {
    /// The shared state.
    inner: Arc<Inner>,
}

impl Client {
    /// The next event, or `None` after the connection closes and the caller has read every event.
    pub async fn event(&self) -> Option<Event> {
        self.inner.events.lock().await.recv().await
    }

    /// Whether the connection has closed.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.inner.commands.is_closed()
    }

    /// Sends one request and waits for the response with its id.
    ///
    /// A `Failure` result still counts as an answer, and the caller decides what it means.
    ///
    /// # Errors
    ///
    /// Returns an error if the connection has closed or closes before the helper answers.
    pub async fn request(&self, op: RequestOp) -> Result<ResponseResult> {
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let (reply, answer) = oneshot::channel();
        self.inner
            .commands
            .send(Command { id, op, reply })
            .await
            .map_err(|_| anyhow!("the control connection to avirtvmd is closed"))?;
        answer.await.map_err(|_| anyhow!("the control connection to avirtvmd closed before it answered"))
    }

    /// Starts the pump that owns `wire`.
    fn start(wire: Wire) -> Self {
        let (commands, command_rx) = mpsc::channel(16);
        let (event_tx, events) = mpsc::unbounded_channel();
        tokio::spawn(pump(wire, command_rx, event_tx));
        Self { inner: Arc::new(Inner { commands, events: Mutex::new(events), next_id: AtomicU64::new(1) }) }
    }
}

/// Dials the helper's socket and runs the handshake.
///
/// The handshake checks the peer's user, then the version range, the workload id, and the
/// incarnation against `expected`. Each mismatch gets a `Refusal` that names it, and the
/// connection closes.
///
/// # Errors
///
/// Returns the [`HandshakeError`] that names why the connection is unusable.
pub async fn connect(
    socket: &Path,
    expected: &Expected<'_>,
    hello_timeout: Duration,
) -> Result<Client, HandshakeError> {
    let stream = UnixStream::connect(socket).await.map_err(HandshakeError::Connect)?;
    let peer = stream.peer_cred().map_err(HandshakeError::Io)?;
    let mut wire = Framed::new(stream, codec());
    if peer.uid() != process::getuid().as_raw() {
        refuse(&mut wire, RefusalCode::WrongPeer, "the peer is not this user's process").await;
        return Err(HandshakeError::WrongPeer);
    }
    let hello = read_hello(&mut wire, hello_timeout).await?;
    if let Err(mismatch) = vet(&hello, expected) {
        refuse(&mut wire, mismatch.code, mismatch.note).await;
        return Err(mismatch.error);
    }
    let answer = HostMessage::from(HostBody::Hello(HostHello { version: PROTOCOL_VERSION }));
    send(&mut wire, &answer).await.map_err(HandshakeError::Io)?;
    Ok(Client::start(wire))
}

/// The length-prefix framing.
fn codec() -> LengthDelimitedCodec {
    LengthDelimitedCodec::builder().big_endian().length_field_length(4).max_frame_length(MAX_FRAME).new_codec()
}

/// Sends one command to the helper and remembers who waits for the answer.
///
/// Returns false when the wire failed.
async fn dispatch(
    wire: &mut Wire,
    command: Command,
    pending: &mut HashMap<u64, oneshot::Sender<ResponseResult>>,
) -> bool {
    let Command { id, op, reply } = command;
    pending.insert(id, reply);
    let request = HostMessage::from(HostBody::Request(Box::new(Request::new(id, op))));
    send(wire, &request).await.is_ok()
}

/// Sends the frame for `message`.
///
/// # Errors
///
/// Returns an error if the connection fails.
async fn send<M>(wire: &mut Wire, message: &M) -> io::Result<()>
where
    M: Message,
{
    wire.send(Bytes::from(message.encode_to_vec())).await
}

/// Tells the helper why avirthostd is hanging up.
///
/// The helper may have exited already, so a failed send doesn't matter.
async fn refuse(wire: &mut Wire, code: RefusalCode, note: &str) {
    let refusal = HostMessage::from(HostBody::Refusal(Refusal { code: code.into(), message: note.to_owned() }));
    drop(time::timeout(Duration::from_secs(1), send(wire, &refusal)).await);
}

/// Reads the helper's first frame, which must be a hello.
///
/// # Errors
///
/// Returns the [`HandshakeError`] for a timeout, a closed connection, or a first frame that isn't a hello.
async fn read_hello(wire: &mut Wire, limit: Duration) -> Result<HelperHello, HandshakeError> {
    let frame = match time::timeout(limit, wire.next()).await {
        Err(_) => return Err(HandshakeError::NoHello),
        Ok(None) => return Err(HandshakeError::Closed),
        Ok(Some(Err(err))) => return Err(HandshakeError::Io(err)),
        Ok(Some(Ok(frame))) => frame,
    };
    match HelperMessage::decode(frame).map(HelperMessage::into_body) {
        Ok(Some(HelperBody::Hello(hello))) => Ok(hello),
        Ok(Some(HelperBody::Event(_) | HelperBody::Refusal(_) | HelperBody::Response(_)) | None) => {
            Err(HandshakeError::Protocol("the first frame isn't a hello".to_owned()))
        }
        Err(err) => Err(HandshakeError::Protocol(err.to_string())),
    }
}

/// Sends what the helper says to its waiting caller, or queues it as an event.
///
/// Returns false when the helper sent something that ends the connection.
fn route(
    frame: &[u8],
    pending: &mut HashMap<u64, oneshot::Sender<ResponseResult>>,
    events: &mpsc::UnboundedSender<Event>,
) -> bool {
    match HelperMessage::decode(frame).map(HelperMessage::into_body) {
        Ok(Some(HelperBody::Response(response))) => {
            if let (Some(reply), Some(result)) = (pending.remove(&response.id), response.into_result()) {
                drop(reply.send(result));
            }
            true
        }
        Ok(Some(HelperBody::Event(event))) => {
            drop(events.send(event));
            true
        }
        Ok(Some(HelperBody::Hello(_) | HelperBody::Refusal(_)) | None) | Err(_) => false,
    }
}

/// Sends each command over the wire and routes what the helper sends back, until either side
/// ends the connection.
async fn pump(mut wire: Wire, mut commands: mpsc::Receiver<Command>, events: mpsc::UnboundedSender<Event>) {
    let mut pending = HashMap::new();
    loop {
        let alive = tokio::select! {
            asked = commands.recv() => match asked {
                Some(command) => dispatch(&mut wire, command, &mut pending).await,
                None => false,
            },
            received = wire.next() => match received {
                Some(Ok(frame)) => route(&frame, &mut pending, &events),
                Some(Err(_)) | None => false,
            },
        };
        if !alive {
            break;
        }
    }
}

/// Checks a hello against what avirthostd expects of this helper.
///
/// # Errors
///
/// Returns the mismatch, with the refusal to send for it.
fn vet(hello: &HelperHello, expected: &Expected<'_>) -> Result<(), Mismatch> {
    if !(hello.min_version..=hello.max_version).contains(&PROTOCOL_VERSION) {
        return Err(Mismatch {
            code: RefusalCode::UnsupportedVersion,
            error: HandshakeError::UnsupportedVersion { max: hello.max_version, min: hello.min_version },
            note: "avirthostd speaks protocol version 1",
        });
    }
    if hello.workload_id != expected.workload {
        return Err(Mismatch {
            code: RefusalCode::WrongWorkload,
            error: HandshakeError::WrongWorkload(hello.workload_id.clone()),
            note: "this is not the helper avirthostd started for the workload",
        });
    }
    if !constant_time_eq(&hello.incarnation, expected.incarnation.as_bytes()) {
        return Err(Mismatch {
            code: RefusalCode::StaleIncarnation,
            error: HandshakeError::StaleIncarnation,
            note: "the incarnation is not the one avirthostd started",
        });
    }
    Ok(())
}
