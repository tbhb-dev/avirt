//! The container's single long-lived HTTP/2 request whose body is the NDJSON actor log.

use std::convert::Infallible;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Result, bail};
use bytes::Bytes;
use futures_util::StreamExt;
use http_body_util::{BodyExt, StreamBody};
use hyper::{Request, StatusCode, body::Frame};
use serde_json::Value;
use tokio::net::UnixStream;
use tokio::sync::{mpsc, oneshot, watch};
use tokio_stream::wrappers::UnboundedReceiverStream;

use crate::envelope::{Dir, Envelope};
use crate::h2c;

/// How one attempt to open the uplink failed.
enum Attempt {
    Retry(String),
    Fatal(String),
}

type Outcome = Option<Result<StatusCode, String>>;

#[derive(Clone, Debug)]
pub struct Uplink {
    /// `None` once closed; sends after that are no-ops.
    tx: Arc<Mutex<Option<mpsc::UnboundedSender<Bytes>>>>,
    outcome: watch::Receiver<Outcome>,
}

impl Uplink {
    /// Connects to the supervisor's actor socket, runs the HTTP/2 client over it and opens
    /// the actor's streaming POST. Returns only once the controller has accepted the stream, so the
    /// agent is never started into a dead uplink. Every step has a timeout: a socket file whose
    /// listener is gone accepts a connect and then says nothing.
    pub async fn open(socket: &Path, actor_id: &str, token: &str, wait: Duration) -> Result<Self> {
        let deadline = tokio::time::Instant::now() + wait;
        loop {
            // Connecting is not proof that anyone is listening: the runtime's relay accepts the
            // connection inside the VM whether or not the host supervisor is there. So the whole
            // sequence is retried, and readiness is the controller's answer to the stream request.
            match Self::attempt(socket, actor_id, token).await {
                Ok(uplink) => return Ok(uplink),
                Err(Attempt::Fatal(why)) => bail!("{why}"),
                Err(Attempt::Retry(why)) if tokio::time::Instant::now() >= deadline => {
                    bail!("the supervisor is not answering on {} ({why})", socket.display());
                }
                Err(Attempt::Retry(_)) => tokio::time::sleep(Duration::from_millis(300)).await,
            }
        }
    }

    async fn attempt(socket: &Path, actor_id: &str, token: &str) -> Result<Self, Attempt> {
        let retry = |why: &str| Attempt::Retry(why.to_owned());
        let conn = match tokio::time::timeout(Duration::from_secs(5), UnixStream::connect(socket)).await {
            Ok(Ok(conn)) => conn,
            Ok(Err(e)) => return Err(Attempt::Retry(e.to_string())),
            Err(_) => return Err(retry("connect timed out")),
        };
        let mut send = match tokio::time::timeout(Duration::from_secs(5), h2c::handshake(conn)).await {
            Ok(Ok(send)) => send,
            Ok(Err(e)) => return Err(Attempt::Retry(e.to_string())),
            Err(_) => return Err(retry("handshake timed out")),
        };
        let (tx, rx) = mpsc::unbounded_channel::<Bytes>();
        let frames = UnboundedReceiverStream::new(rx).map(|b| Ok::<_, Infallible>(Frame::data(b)));
        let body = BodyExt::boxed(StreamBody::new(frames));
        let request = Request::post(format!("http://localhost/actors/{actor_id}/stream"))
            .header("content-type", "application/x-ndjson")
            .header("x-agentvmm-token", token)
            .body(body)
            .map_err(|e| Attempt::Fatal(e.to_string()))?;
        let (outcome_tx, outcome) = watch::channel(None);
        let (started_tx, started_rx) = oneshot::channel::<Result<StatusCode, String>>();
        tokio::spawn(async move {
            let result = match send.send_request(request).await {
                Ok(response) => {
                    let status = response.status();
                    // The controller answers as soon as it accepts or refuses the stream.
                    let _ = started_tx.send(Ok(status));
                    if status == StatusCode::OK {
                        // The body arrives once the controller has drained our request body.
                        let _ = response.into_body().collect().await;
                    }
                    Ok(status)
                }
                Err(err) => {
                    let _ = started_tx.send(Err(err.to_string()));
                    Err(err.to_string())
                }
            };
            let _ = outcome_tx.send(Some(result));
        });
        match tokio::time::timeout(Duration::from_secs(4), started_rx).await {
            Ok(Ok(Ok(StatusCode::OK))) => Ok(Self { tx: Arc::new(Mutex::new(Some(tx))), outcome }),
            // A refusal that retrying cannot fix: wrong token, unknown, repeated or finished actor.
            Ok(Ok(Ok(status))) if matches!(status.as_u16(), 403 | 404 | 409 | 410) => {
                Err(Attempt::Fatal(format!("the controller refused the stream: HTTP {status}")))
            }
            Ok(Ok(Ok(status))) => Err(Attempt::Retry(format!("HTTP {status}"))),
            Ok(Ok(Err(err))) => Err(Attempt::Retry(err)),
            _ => Err(retry("no answer to the stream request")),
        }
    }

    pub fn send(&self, dir: Dir, msg: Value) {
        if let Some(tx) = self.tx.lock().expect("uplink lock").as_ref() {
            let _ = tx.send(Bytes::from(Envelope::new(dir, msg).to_line()));
        }
    }

    pub fn meta(&self, msg: Value) {
        self.send(Dir::Meta, msg);
    }

    /// Resolves with a reason if the controller refuses the stream (404, 409, 410) or it dies
    /// before `close`; stays pending otherwise.
    pub async fn rejected(&self) -> String {
        let mut rx = self.outcome.clone();
        let outcome =
            rx.wait_for(|o| matches!(o, Some(Err(_))) || matches!(o, Some(Ok(s)) if *s != StatusCode::OK)).await;
        match outcome.as_deref() {
            Ok(Some(Ok(status))) => format!("controller refused the stream: HTTP {status}"),
            Ok(Some(Err(err))) => format!("uplink failed: {err}"),
            _ => std::future::pending().await,
        }
    }

    /// Ends the request body and waits (bounded) for the controller to finish draining it. A no-op
    /// wait if the request already completed.
    pub async fn close(&self, timeout: Duration) {
        self.tx.lock().expect("uplink lock").take();
        let mut rx = self.outcome.clone();
        let _ = tokio::time::timeout(timeout, rx.wait_for(Option::is_some)).await;
    }
}
