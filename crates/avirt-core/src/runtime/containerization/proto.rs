//! The control channel's messages, field for field and tag for tag the same as
//! `crates/avirtvmd/Sources/AvirtVMD/Protocol/avirtvmd.proto`.
//!
//! Both sides agree on the encodings in `crates/avirtvmd/Tests/AvirtVMDTests/fixtures`.

#[cfg(test)]
mod tests;

use bytes::{Buf, BufMut};
use prost::encoding::{DecodeContext, WireType, skip_field};
use prost::{DecodeError, Message};

/// Provides [`Message`] for messages without fields. The derive macro doesn't cover them.
macro_rules! empty_message {
    ($($name:ident),+) => {$(
        impl Message for $name {
            fn clear(&mut self) {}

            fn encode_raw(&self, _buf: &mut impl BufMut) {}

            fn encoded_len(&self) -> usize {
                0
            }

            fn merge_field(
                &mut self,
                tag: u32,
                wire_type: WireType,
                buf: &mut impl Buf,
                ctx: DecodeContext,
            ) -> Result<(), DecodeError> {
                skip_field(wire_type, tag, buf, ctx)
            }
        }
    )+};
}

/// The content of a [`HostMessage`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HostBody {
    /// The answer to the helper's hello.
    Hello(HostHello),
    /// The reason avirthostd is hanging up.
    Refusal(Refusal),
    /// A request for the helper to act.
    Request(Box<Request>),
}

/// What avirthostd sends. The fields form a Protocol Buffers `oneof`, so exactly one is present.
#[derive(Clone, Eq, PartialEq, prost::Message)]
pub struct HostMessage {
    /// The answer to the helper's hello.
    #[prost(message, optional, tag = "1")]
    pub hello: Option<HostHello>,
    /// The reason avirthostd is hanging up.
    #[prost(message, optional, tag = "2")]
    pub refusal: Option<Refusal>,
    /// A request for the helper to act.
    #[prost(message, optional, boxed, tag = "3")]
    pub request: Option<Box<Request>>,
}

/// The content of a [`HelperMessage`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HelperBody {
    /// Something that happened to the workload.
    Event(Event),
    /// The helper's first frame.
    Hello(HelperHello),
    /// The reason the helper is hanging up.
    Refusal(Refusal),
    /// The answer to a request.
    Response(Response),
}

/// What avirtvmd sends. The fields form a Protocol Buffers `oneof`, so exactly one is present.
#[derive(Clone, Eq, PartialEq, prost::Message)]
pub struct HelperMessage {
    /// Something that happened to the workload.
    #[prost(message, optional, tag = "4")]
    pub event: Option<Event>,
    /// The helper's first frame.
    #[prost(message, optional, tag = "1")]
    pub hello: Option<HelperHello>,
    /// The reason the helper is hanging up.
    #[prost(message, optional, tag = "2")]
    pub refusal: Option<Refusal>,
    /// The answer to a request.
    #[prost(message, optional, tag = "3")]
    pub response: Option<Response>,
}

/// The helper's first frame on every connection.
#[derive(Clone, Eq, PartialEq, prost::Message)]
pub struct HelperHello {
    /// The 16 bytes avirthostd gave the helper at start.
    #[prost(bytes = "vec", tag = "4")]
    pub incarnation: Vec<u8>,
    /// The newest protocol version the helper speaks.
    #[prost(uint32, tag = "2")]
    pub max_version: u32,
    /// The oldest protocol version the helper speaks.
    #[prost(uint32, tag = "1")]
    pub min_version: u32,
    /// The workload the helper serves.
    #[prost(string, tag = "3")]
    pub workload_id: String,
}

/// avirthostd's answer to the hello: the version it chose.
#[derive(Clone, Copy, Eq, PartialEq, prost::Message)]
pub struct HostHello {
    /// The protocol version for the connection.
    #[prost(uint32, tag = "1")]
    pub version: u32,
}

/// Why one side refuses the other.
///
/// The variants are in alphabetical order, so each field of this type names `Unspecified` as its
/// default. The derive would take the first variant otherwise, and the wire default is zero.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, prost::Enumeration)]
#[repr(i32)]
pub enum RefusalCode {
    /// The peer presented an incarnation other than the one avirthostd started.
    StaleIncarnation = 3,
    /// Nobody set the code.
    Unspecified = 0,
    /// The peer's protocol versions don't include the one this side speaks.
    UnsupportedVersion = 1,
    /// The peer isn't this user's process.
    WrongPeer = 4,
    /// The peer serves another workload.
    WrongWorkload = 2,
}

/// One side's reason for hanging up.
#[derive(Clone, Eq, PartialEq, prost::Message)]
pub struct Refusal {
    /// What kind of refusal this is.
    #[prost(enumeration = "RefusalCode", tag = "1", default = "Unspecified")]
    pub code: i32,
    /// A sentence for the log.
    #[prost(string, tag = "2")]
    pub message: String,
}

/// What a [`Request`] asks for.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RequestOp {
    /// Start the workload's VM.
    StartWorkload(Box<StartWorkload>),
    /// Report the workload's state.
    Status(Status),
    /// Stop the workload and the helper.
    Stop(Stop),
}

/// One request from avirthostd. The operation fields form a Protocol Buffers `oneof`, so exactly one
/// is present.
#[derive(Clone, Eq, PartialEq, prost::Message)]
pub struct Request {
    /// The id the helper repeats in its response.
    #[prost(uint64, tag = "1")]
    pub id: u64,
    /// Start the workload's VM.
    #[prost(message, optional, boxed, tag = "2")]
    pub start_workload: Option<Box<StartWorkload>>,
    /// Report the workload's state.
    #[prost(message, optional, tag = "3")]
    pub status: Option<Status>,
    /// Stop the workload and the helper.
    #[prost(message, optional, tag = "4")]
    pub stop: Option<Stop>,
}

/// The outcome of a [`Request`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResponseResult {
    /// The request failed.
    Failure(Failure),
    /// The workload's VM is running.
    Started(Started),
    /// The workload's state.
    Status(StatusReply),
    /// The workload and the helper are stopping.
    Stopped(Stopped),
}

/// The helper's answer to one [`Request`]. The outcome fields form a Protocol Buffers `oneof`, so at
/// most one is present.
#[derive(Clone, Eq, PartialEq, prost::Message)]
pub struct Response {
    /// The request failed.
    #[prost(message, optional, tag = "5")]
    pub failure: Option<Failure>,
    /// The id of the request this answers.
    #[prost(uint64, tag = "1")]
    pub id: u64,
    /// The workload's VM is running.
    #[prost(message, optional, tag = "2")]
    pub started: Option<Started>,
    /// The workload's state.
    #[prost(message, optional, tag = "3")]
    pub status: Option<StatusReply>,
    /// The workload and the helper are stopping.
    #[prost(message, optional, tag = "4")]
    pub stopped: Option<Stopped>,
}

/// What happened to the workload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EventKind {
    /// The workload container exited.
    WorkloadExited(WorkloadExited),
}

/// Something that happened to the workload without a request. The fields form a Protocol Buffers
/// `oneof`, so exactly one is present.
#[derive(Clone, Eq, PartialEq, prost::Message)]
pub struct Event {
    /// The workload container exited.
    #[prost(message, optional, tag = "1")]
    pub workload_exited: Option<WorkloadExited>,
}

/// Everything the helper needs to start the workload's VM.
#[derive(Clone, Eq, PartialEq, prost::Message)]
pub struct StartWorkload {
    /// `containers/<id>/`, which the helper creates with mode 0700.
    #[prost(string, tag = "5")]
    pub container_dir: String,
    /// `KEY=VALUE`, appended to the image's environment.
    #[prost(string, repeated, tag = "7")]
    pub env: Vec<String>,
    /// The OCI image config blob in `images/`.
    #[prost(string, tag = "6")]
    pub image_config_path: String,
    /// The initial filesystem for the VM.
    #[prost(string, tag = "3")]
    pub initfs_path: String,
    /// The Linux kernel for the VM.
    #[prost(string, tag = "2")]
    pub kernel_path: String,
    /// A read-only cache entry that the helper clones.
    #[prost(string, tag = "4")]
    pub rootfs_base_path: String,
    /// Host directories shared into the container.
    #[prost(message, repeated, tag = "8")]
    pub shares: Vec<Share>,
    /// Host sockets presented inside the container.
    #[prost(message, repeated, tag = "9")]
    pub sockets: Vec<Socket>,
    /// The sha256 of this message with `env` cleared.
    #[prost(string, tag = "1")]
    pub spec_digest: String,
    /// Where the helper writes the container's stdout and stderr.
    #[prost(string, tag = "10")]
    pub stdio_log_path: String,
}

/// A host directory shared into the container.
#[derive(Clone, Eq, PartialEq, prost::Message)]
pub struct Share {
    /// Where it appears in the container.
    #[prost(string, tag = "2")]
    pub guest_path: String,
    /// The directory on the host.
    #[prost(string, tag = "1")]
    pub host_path: String,
}

/// A socket on the host that the container reaches through a file at `guest_path`.
#[derive(Clone, Eq, PartialEq, prost::Message)]
pub struct Socket {
    /// Where it appears in the container.
    #[prost(string, tag = "2")]
    pub guest_path: String,
    /// The socket on the host.
    #[prost(string, tag = "1")]
    pub host_path: String,
    /// The file mode inside the container.
    #[prost(uint32, tag = "3")]
    pub mode: u32,
}

/// The answer to a successful `StartWorkload`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Started;

/// Asks for the workload's state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Status;

/// Where the workload stands.
///
/// The variants are in alphabetical order, so each field of this type names `Unspecified` as its
/// default. The derive would take the first variant otherwise, and the wire default is zero.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, prost::Enumeration)]
#[repr(i32)]
pub enum State {
    /// The workload exited.
    Exited = 4,
    /// No workload has started.
    Idle = 1,
    /// The workload is running.
    Running = 3,
    /// The workload's VM is starting.
    Starting = 2,
    /// The workload is stopping.
    Stopping = 5,
    /// Nobody set the state.
    Unspecified = 0,
}

/// The answer to a [`Status`] request.
#[derive(Clone, Eq, PartialEq, prost::Message)]
pub struct StatusReply {
    /// How the workload ended, set when the state is [`State::Exited`].
    #[prost(message, optional, tag = "2")]
    pub exit: Option<ExitStatus>,
    /// The workload's state.
    #[prost(enumeration = "State", tag = "1", default = "Unspecified")]
    pub state: i32,
}

/// Asks the helper to stop the workload and exit.
#[derive(Clone, Copy, Eq, PartialEq, prost::Message)]
pub struct Stop {
    /// How long the workload has to stop before the helper forces it.
    #[prost(uint32, tag = "1")]
    pub grace_seconds: u32,
}

/// The helper's reply to [`Stop`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Stopped;

/// The answer to a request that the helper couldn't complete.
#[derive(Clone, Eq, PartialEq, prost::Message)]
pub struct Failure {
    /// What went wrong, for the log.
    #[prost(string, tag = "1")]
    pub message: String,
}

/// The workload container exited.
#[derive(Clone, Eq, PartialEq, prost::Message)]
pub struct WorkloadExited {
    /// How it ended.
    #[prost(message, optional, tag = "1")]
    pub exit: Option<ExitStatus>,
}

/// How a workload container ended.
#[derive(Clone, Eq, PartialEq, prost::Message)]
pub struct ExitStatus {
    /// The exit code, if the container exited normally.
    #[prost(int32, optional, tag = "1")]
    pub code: Option<i32>,
    /// Why the helper couldn't say how the container ended, if it couldn't.
    #[prost(string, tag = "3")]
    pub error: String,
    /// The signal that ended the container, if one did.
    #[prost(int32, optional, tag = "2")]
    pub signal: Option<i32>,
}

impl HostMessage {
    /// The message's content, if a field is present.
    #[must_use]
    pub fn into_body(self) -> Option<HostBody> {
        let Self { hello, refusal, request } = self;
        hello.map(HostBody::Hello).or_else(|| refusal.map(HostBody::Refusal)).or_else(|| request.map(HostBody::Request))
    }
}

impl From<HostBody> for HostMessage {
    fn from(body: HostBody) -> Self {
        let mut message = Self { hello: None, refusal: None, request: None };
        match body {
            HostBody::Hello(hello) => message.hello = Some(hello),
            HostBody::Refusal(refusal) => message.refusal = Some(refusal),
            HostBody::Request(request) => message.request = Some(request),
        }
        message
    }
}

impl HelperMessage {
    /// The message's content, if a field is present.
    #[must_use]
    pub fn into_body(self) -> Option<HelperBody> {
        let Self { event, hello, refusal, response } = self;
        hello
            .map(HelperBody::Hello)
            .or_else(|| refusal.map(HelperBody::Refusal))
            .or_else(|| response.map(HelperBody::Response))
            .or_else(|| event.map(HelperBody::Event))
    }
}

impl From<HelperBody> for HelperMessage {
    fn from(body: HelperBody) -> Self {
        let mut message = Self { event: None, hello: None, refusal: None, response: None };
        match body {
            HelperBody::Event(event) => message.event = Some(event),
            HelperBody::Hello(hello) => message.hello = Some(hello),
            HelperBody::Refusal(refusal) => message.refusal = Some(refusal),
            HelperBody::Response(response) => message.response = Some(response),
        }
        message
    }
}

impl Request {
    /// The operation, if a field is present.
    #[must_use]
    pub fn into_op(self) -> Option<RequestOp> {
        let Self { start_workload, status, stop, .. } = self;
        start_workload
            .map(RequestOp::StartWorkload)
            .or_else(|| status.map(RequestOp::Status))
            .or_else(|| stop.map(RequestOp::Stop))
    }

    /// A request with an id and an operation.
    #[must_use]
    pub fn new(id: u64, op: RequestOp) -> Self {
        let mut request = Self { id, start_workload: None, status: None, stop: None };
        match op {
            RequestOp::StartWorkload(start) => request.start_workload = Some(start),
            RequestOp::Status(status) => request.status = Some(status),
            RequestOp::Stop(stop) => request.stop = Some(stop),
        }
        request
    }
}

impl Response {
    /// The outcome, if a field is present.
    #[must_use]
    pub fn into_result(self) -> Option<ResponseResult> {
        let Self { failure, started, status, stopped, .. } = self;
        started
            .map(ResponseResult::Started)
            .or_else(|| status.map(ResponseResult::Status))
            .or_else(|| stopped.map(ResponseResult::Stopped))
            .or_else(|| failure.map(ResponseResult::Failure))
    }

    /// A response to request `id`.
    #[must_use]
    pub fn new(id: u64, result: ResponseResult) -> Self {
        let mut response = Self { failure: None, id, started: None, status: None, stopped: None };
        match result {
            ResponseResult::Failure(failure) => response.failure = Some(failure),
            ResponseResult::Started(started) => response.started = Some(started),
            ResponseResult::Status(status) => response.status = Some(status),
            ResponseResult::Stopped(stopped) => response.stopped = Some(stopped),
        }
        response
    }
}

impl Event {
    /// What happened, if a field is present.
    #[must_use]
    pub fn into_kind(self) -> Option<EventKind> {
        self.workload_exited.map(EventKind::WorkloadExited)
    }
}

impl From<EventKind> for Event {
    fn from(kind: EventKind) -> Self {
        match kind {
            EventKind::WorkloadExited(exited) => Self { workload_exited: Some(exited) },
        }
    }
}

empty_message!(Started, Status, Stopped);
