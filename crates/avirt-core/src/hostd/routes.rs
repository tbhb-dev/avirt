//! The control API and the per-workload stream API, as HTTP routes.

use super::{Hostd, ShuttingDown, StreamTask, mediator::constant_time_eq, profile::Refused};
use crate::api::{HEALTH_PATH, LAUNCH_PATH, LaunchRequest};
use crate::h2c::Body;
use alloc::sync::Arc;
use anyhow::Result;
use bytes::Bytes;
use core::convert::Infallible;
use futures_util::StreamExt as _;
use http_body_util::{BodyExt, Full, Limited, StreamBody};
use hyper::body::{Frame, Incoming};
use hyper::header::{CONTENT_TYPE, HeaderValue};
use hyper::server::conn::http2;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::{TokioExecutor, TokioIo};
use serde_json::{Value, json};
use std::path::PathBuf;
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;

/// Upper bound on a `/launch` request body.
const MAX_LAUNCH_BODY: usize = 64 * 1024;

/// An error reply with a JSON `{"error": message}` body.
pub(super) fn error(status: StatusCode, message: &str) -> Response<Body> {
    reply(status, &json!({ "error": message }))
}

/// The control API, served on the loopback port for the command-line interface.
///
/// # Errors
///
/// Never fails: every problem becomes an error reply.
pub(super) async fn handle_control(hostd: Arc<Hostd>, req: Request<Incoming>) -> Result<Response<Body>, Infallible> {
    Ok(match (req.method(), req.uri().path()) {
        (&Method::POST, LAUNCH_PATH) => launch_route(&hostd, req).await,
        (&Method::POST, HEALTH_PATH) => reply(StatusCode::OK, &json!({ "ok": true })),
        _ => error(StatusCode::NOT_FOUND, "not found"),
    })
}

/// Allows a workload's connection to request only its own stream. A container can't
/// write into another workload's log or launch anything.
fn handle_workload(hostd: &Arc<Hostd>, id: &str, req: Request<Incoming>) -> Response<Body> {
    match (req.method(), req.uri().path()) {
        (&Method::POST, path) if path == format!("/workloads/{id}/stream") => stream_route(hostd, id, req),
        _ => error(StatusCode::NOT_FOUND, "not found"),
    }
}

/// Turns a launch result into the reply to `POST /launch`.
fn launch_reply(hostd: &Hostd, launched: Result<String>) -> Response<Body> {
    match launched {
        Ok(id) => {
            let log_path = hostd.log_path(&id);
            // The fields of `LaunchResponse`, in the order the wire has always had.
            reply(StatusCode::OK, &json!({ "workloadId": id, "logPath": log_path.to_string_lossy() }))
        }
        Err(err) if err.downcast_ref::<Refused>().is_some() => error(StatusCode::BAD_REQUEST, &err.to_string()),
        Err(err) if err.downcast_ref::<ShuttingDown>().is_some() => {
            error(StatusCode::SERVICE_UNAVAILABLE, "avirthostd is shutting down")
        }
        Err(err) => {
            eprintln!("launch failed: {err:#}");
            error(StatusCode::INTERNAL_SERVER_ERROR, "launch failed; see avirthostd's output")
        }
    }
}

/// Handles `POST /launch`.
async fn launch_route(hostd: &Arc<Hostd>, req: Request<Incoming>) -> Response<Body> {
    // An aborted request body or one over the limit returns an error here without leaving a handler
    // pending. Buffering always has a limit.
    let Ok(collected) = Limited::new(req.into_body(), MAX_LAUNCH_BODY).collect().await else {
        return error(StatusCode::BAD_REQUEST, "request body aborted or too large");
    };
    let Ok(launch) = serde_json::from_slice::<LaunchRequest>(&collected.to_bytes()) else {
        return error(StatusCode::BAD_REQUEST, "body must be JSON");
    };
    let cwd = match validate_launch(hostd, &launch) {
        Ok(cwd) => cwd,
        Err(message) => return error(StatusCode::BAD_REQUEST, &message),
    };
    let launched = hostd.launch(&launch.agent, &launch.prompt, cwd.as_deref()).await;
    launch_reply(hostd, launched)
}

/// Builds a reply with a JSON body.
fn reply(status: StatusCode, body: &Value) -> Response<Body> {
    let mut response = Response::new(Full::new(Bytes::from(body.to_string())).boxed());
    *response.status_mut() = status;
    response.headers_mut().insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    response
}

/// Serves a workload's HTTP/2 connection, which only allows its own stream.
pub(super) async fn serve_workload(hostd: Arc<Hostd>, conn: UnixStream, id: &str) {
    let owner = Arc::new(id.to_owned());
    let service = service_fn(move |req| {
        let (shared, workload) = (Arc::clone(&hostd), Arc::clone(&owner));
        async move { Ok::<_, Infallible>(handle_workload(&shared, &workload, req)) }
    });
    let served = http2::Builder::new(TokioExecutor::new()).serve_connection(TokioIo::new(conn), service);
    if let Err(err) = served.await {
        eprintln!("h2 connection error: {err}");
    }
}

/// Handles a workload's stream POST.
fn stream_route(hostd: &Arc<Hostd>, id: &str, req: Request<Incoming>) -> Response<Body> {
    let workload = match hostd.lookup(id) {
        Ok(workload) => workload,
        Err((status, message)) => return error(status, message),
    };
    let presented = req.headers().get("x-avirt-token").and_then(|value| value.to_str().ok()).unwrap_or_default();
    if !constant_time_eq(presented.as_bytes(), workload.token.as_bytes()) {
        return error(StatusCode::FORBIDDEN, "bad workload token");
    }
    if let Err((status, message)) = workload.claim_stream() {
        return error(status, message);
    }
    let (done_tx, done_rx) = mpsc::unbounded_channel::<Bytes>();
    let task = StreamTask {
        done_tx,
        // Keeps shutdown waiting until this stream has drained into the log.
        drain: hostd.tracker.token(),
        hostd: Arc::clone(hostd),
        id: id.to_owned(),
        workload,
    };

    // Send response headers now so avirtinitd learns at once that avirthostd accepted the stream.
    // The body (`{"lines": N}`) follows once the request body has drained.
    tokio::spawn(task.run(req.into_body()));

    let frames = UnboundedReceiverStream::new(done_rx).map(|bytes| Ok::<_, Infallible>(Frame::data(bytes)));
    let mut response = Response::new(BodyExt::boxed(StreamBody::new(frames)));
    response.headers_mut().insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    response
}

/// Checks a launch request and returns its working directory.
///
/// # Errors
///
/// Returns the message for a 400 when the agent is unknown, the prompt is empty, or the working
/// directory isn't an absolute path.
fn validate_launch(hostd: &Hostd, body: &LaunchRequest) -> Result<Option<PathBuf>, String> {
    if !hostd.profiles.contains_key(body.agent.as_str()) {
        let mut names: Vec<_> = hostd.profiles.keys().map(String::as_str).collect();
        names.sort_unstable();
        return Err(format!("agent must be one of: {}", names.join(", ")));
    }
    if body.prompt.is_empty() {
        return Err("prompt is required".to_owned());
    }
    match &body.cwd {
        None => Ok(None),
        // `:` would end the host side of the `-v host:container` mount spec.
        Some(cwd) if cwd.starts_with('/') && !cwd.contains(':') => Ok(Some(PathBuf::from(cwd))),
        Some(_) => Err("cwd must be an absolute path without ':'".to_owned()),
    }
}
