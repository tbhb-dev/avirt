//! A stand-in for avirtvmd, for tests of the avirthostd side of the control channel.

use super::lifecycle::Incarnation;
use super::proto::{
    Event, EventKind, ExitStatus, Failure, HelperBody, HelperHello, HelperMessage, HostBody, HostMessage, Refusal,
    RefusalCode, Request, RequestOp, Response, ResponseResult, Started, State, StatusReply, Stopped, WorkloadExited,
};
use alloc::sync::Arc;
use anyhow::Result;
use bytes::Bytes;
use futures_util::{SinkExt as _, StreamExt as _};
use prost::Message as _;
use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};
use tokio::net::{UnixListener, UnixStream};
use tokio::task::{JoinHandle, JoinSet};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

/// What the fake received, shared with its tasks.
type Received = Arc<Mutex<Vec<HostMessage>>>;

/// What the fake does with a `StartWorkload` request.
#[derive(Clone, Copy, Eq, PartialEq)]
enum Starts {
    /// Answer `Started`.
    Answer,
    /// Never answer.
    Ignore,
    /// Answer `Failure`.
    Refuse,
}

/// Listens on a socket like a helper does.
///
/// With a hello it sends one on every connection, and without one it never speaks. It records
/// every frame it receives and answers every request. `Status` gets an idle state followed by a
/// `WorkloadExited` event, `Stop` gets `Stopped`, and anything else gets `Started`.
#[derive(Debug)]
pub(super) struct FakeHelper {
    /// Every message the fake received.
    received: Received,
    /// The accept loop, which owns the connections.
    task: JoinHandle<()>,
}

impl FakeHelper {
    /// Binds `socket` and accepts connections.
    ///
    /// # Errors
    ///
    /// Returns an error if binding the socket fails.
    fn bind(socket: &Path, hello: Option<HelperHello>, starts: Starts) -> Result<Self> {
        let listener = UnixListener::bind(socket)?;
        let received = Received::default();
        let task = tokio::spawn(accept(listener, hello, Arc::clone(&received), starts));
        Ok(Self { received, task })
    }

    /// Every message received so far, in order.
    #[must_use]
    pub(super) fn received(&self) -> Vec<HostMessage> {
        guard(&self.received).clone()
    }

    /// Starts the fake on `socket`.
    ///
    /// # Errors
    ///
    /// Returns an error if binding the socket fails.
    pub(super) fn spawn(socket: &Path, hello: Option<HelperHello>) -> Result<Self> {
        Self::bind(socket, hello, Starts::Answer)
    }

    /// Starts the fake on `socket`, never answering a `StartWorkload`.
    ///
    /// # Errors
    ///
    /// Returns an error if binding the socket fails.
    pub(super) fn spawn_ignoring_starts(socket: &Path, hello: Option<HelperHello>) -> Result<Self> {
        Self::bind(socket, hello, Starts::Ignore)
    }

    /// Starts a fake on `socket` that answers every connection with a refusal in place of a hello.
    ///
    /// # Errors
    ///
    /// Returns an error if binding the socket fails.
    pub(super) fn spawn_refusing_peers(socket: &Path, code: RefusalCode, message: &str) -> Result<Self> {
        let listener = UnixListener::bind(socket)?;
        let refusal = Refusal { code: code.into(), message: message.to_owned() };
        let task = tokio::spawn(refuse_all(listener, refusal));
        Ok(Self { received: Received::default(), task })
    }

    /// Starts the fake on `socket`, answering every `StartWorkload` with a `Failure`.
    ///
    /// # Errors
    ///
    /// Returns an error if binding the socket fails.
    pub(super) fn spawn_refusing_starts(socket: &Path, hello: Option<HelperHello>) -> Result<Self> {
        Self::bind(socket, hello, Starts::Refuse)
    }
}

impl Drop for FakeHelper {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Builds the hello for `workload` and `incarnation`, claiming protocol versions `min` through `max`.
#[must_use]
pub(super) fn hello(workload: &str, incarnation: &Incarnation, min: u32, max: u32) -> HelperHello {
    HelperHello {
        incarnation: incarnation.as_bytes().to_vec(),
        max_version: max,
        min_version: min,
        workload_id: workload.to_owned(),
    }
}

/// Takes the lock on what the fake received.
fn guard(received: &Received) -> MutexGuard<'_, Vec<HostMessage>> {
    received.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Accepts connections until something aborts the task. The set of connections drops with the task and
/// takes them down with it.
async fn accept(listener: UnixListener, hello: Option<HelperHello>, received: Received, starts: Starts) {
    let mut connections = JoinSet::new();
    while let Ok((stream, _)) = listener.accept().await {
        connections.spawn(converse(stream, hello.clone(), Arc::clone(&received), starts));
    }
}

/// Answers every connection with `refusal` and keeps it open until the peer hangs up. The set of
/// connections drops with the task and takes them down with it.
async fn refuse_all(listener: UnixListener, refusal: Refusal) {
    let mut connections = JoinSet::new();
    while let Ok((stream, _)) = listener.accept().await {
        let message = HelperMessage::from(HelperBody::Refusal(refusal.clone()));
        connections.spawn(hold_open(stream, message));
    }
}

/// Sends `message` and then reads until the peer hangs up.
async fn hold_open(stream: UnixStream, message: HelperMessage) {
    let mut wire = Framed::new(stream, LengthDelimitedCodec::new());
    if wire.send(Bytes::from(message.encode_to_vec())).await.is_ok() {
        while wire.next().await.is_some() {}
    }
}

/// Sends the hello, then records and answers every frame.
async fn converse(stream: UnixStream, hello: Option<HelperHello>, received: Received, starts: Starts) {
    let mut wire = Framed::new(stream, LengthDelimitedCodec::new());
    if let Some(greeting) = hello {
        let message = HelperMessage::from(HelperBody::Hello(greeting));
        if wire.send(Bytes::from(message.encode_to_vec())).await.is_err() {
            return;
        }
    }
    while let Some(Ok(frame)) = wire.next().await {
        let Ok(message) = HostMessage::decode(frame) else { return };
        guard(&received).push(message.clone());
        if let Some(HostBody::Request(request)) = message.into_body()
            && reply(&mut wire, *request, starts).await.is_err()
        {
            return;
        }
    }
}

/// Answers one request, and announces an exit after a status.
///
/// # Errors
///
/// Returns an error if the connection fails.
async fn reply(wire: &mut Framed<UnixStream, LengthDelimitedCodec>, request: Request, starts: Starts) -> Result<()> {
    let id = request.id;
    let (result, announce) = match request.into_op() {
        Some(RequestOp::Status(_)) => {
            (ResponseResult::Status(StatusReply { exit: None, state: State::Idle.into() }), true)
        }
        Some(RequestOp::Stop(_)) => (ResponseResult::Stopped(Stopped), false),
        Some(RequestOp::StartWorkload(_)) if starts == Starts::Refuse => {
            (ResponseResult::Failure(Failure { message: "the VM can't boot".to_owned() }), false)
        }
        Some(RequestOp::StartWorkload(_)) if starts == Starts::Ignore => return Ok(()),
        Some(RequestOp::StartWorkload(_)) | None => (ResponseResult::Started(Started), false),
    };
    let answer = HelperMessage::from(HelperBody::Response(Response::new(id, result)));
    wire.send(Bytes::from(answer.encode_to_vec())).await?;
    if announce {
        let exit = ExitStatus { code: Some(0i32), error: String::new(), signal: None };
        let exited = Event::from(EventKind::WorkloadExited(WorkloadExited { exit: Some(exit) }));
        wire.send(Bytes::from(HelperMessage::from(HelperBody::Event(exited)).encode_to_vec())).await?;
    }
    Ok(())
}
