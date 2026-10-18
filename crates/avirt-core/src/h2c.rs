//! Client helpers for HTTP/2 with prior knowledge and without TLS.

use core::convert::Infallible;

use anyhow::{Context as _, Result, anyhow};
use bytes::Bytes;
use http_body_util::{BodyExt as _, Full, combinators::BoxBody};
use hyper::client::conn::http2::{self, SendRequest};
use hyper::{Request, StatusCode, Uri};
use hyper_util::rt::{TokioExecutor, TokioIo};
use serde_json::Value;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;

/// The boxed request and response body type used on every avirt HTTP/2 connection.
pub type Body = BoxBody<Bytes, Infallible>;

/// Runs an HTTP/2 client handshake over any connected byte stream.
///
/// The connection driver runs in the background and reports a connection error once instead of
/// leaving it unhandled. Which side opened the transport is irrelevant to HTTP/2.
///
/// # Errors
///
/// Returns an error if the HTTP/2 handshake fails.
pub async fn handshake<IO>(io: IO) -> Result<SendRequest<Body>>
where
    IO: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    let (send, conn) = http2::handshake(TokioExecutor::new(), TokioIo::new(io)).await?;
    tokio::spawn(async move {
        if let Err(err) = conn.await {
            eprintln!("mediator connection error: {err}");
        }
    });
    Ok(send)
}

/// Sends a JSON body in a POST to `http://host:port{path}` and returns the status with the parsed JSON
/// response.
///
/// The command-line interface uses this for avirthostd's loopback control port.
///
/// # Errors
///
/// Returns an error if the URL is invalid, the connection or request fails, or the response body
/// isn't JSON.
pub async fn post_json(url: &str, path: &str, body: &Value) -> Result<(StatusCode, Value)> {
    let uri: Uri = url.parse().with_context(|| format!("invalid URL {url}"))?;
    let authority = uri.authority().ok_or_else(|| anyhow!("URL {url} has no host:port"))?.to_string();
    let tcp = TcpStream::connect(&authority).await.with_context(|| format!("connect {authority}"))?;
    let mut send = handshake(tcp).await?;
    let target = Uri::builder().scheme("http").authority(authority).path_and_query(path).build()?;
    let payload = Full::new(Bytes::from(serde_json::to_vec(body)?)).boxed();
    let request = Request::post(target).header("content-type", "application/json").body(payload)?;
    let response = send.send_request(request).await?;
    let status = response.status();
    let bytes = response.into_body().collect().await?.to_bytes();
    Ok((status, serde_json::from_slice(&bytes).context("avirthostd returned a non-JSON body")?))
}
