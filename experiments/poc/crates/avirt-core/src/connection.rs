//! The container's single long-lived HTTP/2 request whose body is the NDJSON workload log.

use alloc::sync::Arc;
use core::convert::Infallible;
use core::future;
use core::time::Duration;
use std::path::Path;
use std::sync::{Mutex, PoisonError};

use anyhow::{Result, bail};
use bytes::Bytes;
use futures_util::StreamExt as _;
use http_body_util::{BodyExt, StreamBody};
use hyper::body::Frame;
use hyper::client::conn::http2::SendRequest;
use hyper::{Request, StatusCode};
use serde_json::Value;
use tokio::net::UnixStream;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::{self, Instant};
use tokio_stream::wrappers::UnboundedReceiverStream;

use crate::envelope::{Dir, Envelope};
use crate::h2c::{self, Body};

/// How one attempt to open the mediator connection failed.
enum Attempt {
    /// A failure that retrying can't fix.
    Fatal(String),
    /// A failure that the next attempt may not repeat.
    Retry(String),
}

/// The streaming request's result: `None` while it runs, then its status or the transport error.
type Outcome = Option<Result<StatusCode, String>>;

/// The sending half of the request body, shared so `close` can end the body.
type BodySender = Arc<Mutex<Option<mpsc::UnboundedSender<Bytes>>>>;

/// The workload's streaming connection to avirthostd, through the mediator.
#[derive(Clone, Debug)]
pub struct LogStream {
    /// How the streaming request ended, if it has.
    outcome: watch::Receiver<Outcome>,
    /// `None` once closed. Sends after that are no-ops.
    tx: BodySender,
}

impl LogStream {
    /// Makes one attempt to connect, send the stream request, and hear avirthostd's answer.
    ///
    /// # Errors
    ///
    /// Returns [`Attempt::Fatal`] for a refusal that retrying can't fix and [`Attempt::Retry`]
    /// for any other failure.
    async fn attempt(socket: &Path, workload_id: &str, token: &str) -> Result<Self, Attempt> {
        let send = connect(socket).await?;
        let (tx, rx) = mpsc::unbounded_channel::<Bytes>();
        let frames = UnboundedReceiverStream::new(rx).map(|bytes| Ok::<_, Infallible>(Frame::data(bytes)));
        let request = Request::post(format!("http://localhost/workloads/{workload_id}/stream"))
            .header("content-type", "application/x-ndjson")
            .header("x-avirt-token", token)
            .body(BodyExt::boxed(StreamBody::new(frames)))
            .map_err(|err| Attempt::Fatal(err.to_string()))?;
        let (outcome_tx, outcome) = watch::channel(None);
        let (started_tx, started_rx) = oneshot::channel::<Result<StatusCode, String>>();
        tokio::spawn(async move {
            let result = run_request(send, request, started_tx).await;
            drop(outcome_tx.send(Some(result)));
        });
        match time::timeout(Duration::from_secs(4), started_rx).await {
            Ok(Ok(Ok(StatusCode::OK))) => Ok(Self { outcome, tx: Arc::new(Mutex::new(Some(tx))) }),
            // A refusal that retrying can't fix: a wrong token or an unknown, repeated, or finished workload.
            Ok(Ok(Ok(status))) if matches!(status.as_u16(), 403 | 404 | 409 | 410) => {
                Err(Attempt::Fatal(format!("avirthostd refused the stream: HTTP {status}")))
            }
            Ok(Ok(Ok(status))) => Err(Attempt::Retry(format!("HTTP {status}"))),
            Ok(Ok(Err(err))) => Err(Attempt::Retry(err)),
            _ => Err(Attempt::Retry("no answer to the stream request".to_owned())),
        }
    }

    /// Ends the request body and waits (bounded) for avirthostd to finish draining it.
    ///
    /// The wait is a no-op if the request already completed.
    pub async fn close(&self, timeout: Duration) {
        self.tx.lock().unwrap_or_else(PoisonError::into_inner).take();
        let mut outcome = self.outcome.clone();
        drop(time::timeout(timeout, outcome.wait_for(Option::is_some)).await);
    }

    /// Sends a `meta` event to the workload log.
    pub fn meta(&self, msg: Value) {
        self.send(Dir::Meta, msg);
    }

    /// Connects to the mediator's workload socket, runs the HTTP/2 client over it, and opens
    /// the workload's streaming POST.
    ///
    /// Returns only once avirthostd has accepted the stream. The harness never starts the
    /// agent with a dead connection. A timeout applies to each step: a socket file whose
    /// listener no longer exists accepts a connect and then says nothing.
    ///
    /// # Errors
    ///
    /// Returns an error if avirthostd refuses the stream, or if the mediator doesn't answer
    /// within `wait`.
    pub async fn open(socket: &Path, workload_id: &str, token: &str, wait: Duration) -> Result<Self> {
        let started = Instant::now();
        loop {
            // Connecting isn't proof that anyone is listening: the runtime's relay accepts the
            // connection inside the VM even if the host mediator isn't there. Retry the whole
            // sequence and use avirthostd's answer to the stream request as the readiness check.
            match Self::attempt(socket, workload_id, token).await {
                Ok(connection) => return Ok(connection),
                Err(Attempt::Fatal(why)) => bail!("{why}"),
                Err(Attempt::Retry(why)) if started.elapsed() >= wait => {
                    bail!("the mediator is not answering on {} ({why})", socket.display());
                }
                Err(Attempt::Retry(_)) => time::sleep(Duration::from_millis(300)).await,
            }
        }
    }

    /// Resolves with a reason if avirthostd refuses the stream (404, 409, 410) or stops
    /// before `close`. Stays pending otherwise.
    pub async fn rejected(&self) -> String {
        let mut outcome = self.outcome.clone();
        let refusal = {
            let failed = outcome
                .wait_for(|ended| {
                    matches!(ended, Some(Err(_))) || matches!(ended, Some(Ok(status)) if *status != StatusCode::OK)
                })
                .await;
            match failed.as_deref() {
                Ok(Some(Ok(status))) => Some(format!("avirthostd refused the stream: HTTP {status}")),
                Ok(Some(Err(err))) => Some(format!("mediator connection failed: {err}")),
                _ => None,
            }
        };
        match refusal {
            Some(reason) => reason,
            None => future::pending().await,
        }
    }

    /// Sends one envelope with this direction and message to the workload log.
    pub fn send(&self, dir: Dir, msg: Value) {
        if let Some(sender) = self.tx.lock().unwrap_or_else(PoisonError::into_inner).as_ref() {
            drop(sender.send(Bytes::from(Envelope::new(dir, msg).to_line())));
        }
    }
}

/// Connects to the mediator socket and finishes the HTTP/2 handshake, each step under a timeout.
///
/// # Errors
///
/// Returns [`Attempt::Retry`] if either step fails or times out.
async fn connect(socket: &Path) -> Result<SendRequest<Body>, Attempt> {
    let conn = match time::timeout(Duration::from_secs(5), UnixStream::connect(socket)).await {
        Ok(Ok(conn)) => conn,
        Ok(Err(err)) => return Err(Attempt::Retry(err.to_string())),
        Err(_) => return Err(Attempt::Retry("connect timed out".to_owned())),
    };
    match time::timeout(Duration::from_secs(5), h2c::handshake(conn)).await {
        Ok(Ok(send)) => Ok(send),
        Ok(Err(err)) => Err(Attempt::Retry(err.to_string())),
        Err(_) => Err(Attempt::Retry("handshake timed out".to_owned())),
    }
}

/// Sends the streaming request and returns its result.
///
/// Reports avirthostd's first answer on `started`.
///
/// # Errors
///
/// Returns the transport error text if the request fails.
async fn run_request(
    mut send: SendRequest<Body>,
    request: Request<Body>,
    started: oneshot::Sender<Result<StatusCode, String>>,
) -> Result<StatusCode, String> {
    match send.send_request(request).await {
        Ok(response) => {
            let status = response.status();
            // avirthostd answers as soon as it accepts or refuses the stream.
            drop(started.send(Ok(status)));
            if status == StatusCode::OK {
                // The body arrives once avirthostd has drained the request body.
                drop(response.into_body().collect().await);
            }
            Ok(status)
        }
        Err(err) => {
            drop(started.send(Err(err.to_string())));
            Err(err.to_string())
        }
    }
}
