//! Unit tests for the handshake and the connection, against a fake helper.

use super::super::lifecycle::Incarnation;
use super::super::proto::{
    HelperHello, HostBody, HostHello, HostMessage, RefusalCode, RequestOp, ResponseResult, State, Status, StatusReply,
};
use super::super::testing::{FakeHelper, hello};
use super::{Client, Expected, HandshakeError, MAX_FRAME, PROTOCOL_VERSION, connect};
use anyhow::{Context as _, Result, bail, ensure};
use core::time::Duration;
use std::path::Path;
use tokio::io::AsyncWriteExt as _;
use tokio::net::UnixListener;
use tokio::task;
use tokio::time;

/// How long a test waits on a helper that doesn't speak.
const SHORT: Duration = Duration::from_millis(300);

/// The incarnation the tests expect.
///
/// # Errors
///
/// Fails when the test incarnation doesn't parse.
fn incarnation() -> Result<Incarnation> {
    Incarnation::from_hex("000102030405060708090a0b0c0d0e0f").context("the test incarnation")
}

/// Connects as avirthostd would for workload `1a2b3c4d`.
///
/// # Errors
///
/// Fails when the expectation fails to build. The handshake's own result is the inner one.
async fn handshake(socket: &Path, hello_timeout: Duration) -> Result<Result<Client, HandshakeError>> {
    let expected_incarnation = incarnation()?;
    let expected = Expected { incarnation: &expected_incarnation, workload: "1a2b3c4d" };
    Ok(connect(socket, &expected, hello_timeout).await)
}

/// Runs a handshake that must fail against a fake helper that sends `greeting`.
///
/// # Errors
///
/// Fails when the fake can't start or the handshake succeeds.
async fn refused_by(greeting: Option<HelperHello>) -> Result<(HandshakeError, FakeHelper)> {
    let dir = tempfile::tempdir()?;
    let socket = dir.path().join("v.sock");
    let fake = FakeHelper::spawn(&socket, greeting)?;
    match handshake(&socket, SHORT).await? {
        Err(err) => Ok((err, fake)),
        Ok(_) => bail!("the handshake succeeded"),
    }
}

/// Waits for the fake to receive a refusal with `code` as its first message.
///
/// # Errors
///
/// Fails when no refusal arrives or it has another code.
async fn expect_refusal(fake: &FakeHelper, code: RefusalCode) -> Result<()> {
    for _ in 0u8..50 {
        if let Some(HostBody::Refusal(refusal)) = fake.received().into_iter().next().and_then(HostMessage::into_body) {
            ensure!(refusal.code == i32::from(code), "the refusal has code {}", refusal.code);
            return Ok(());
        }
        time::sleep(Duration::from_millis(20)).await;
    }
    bail!("the helper never received a refusal")
}

/// The client answers a matching hello with its own, and then requests and events flow.
///
/// # Errors
///
/// Fails when the handshake, the request, or the event breaks an expectation.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_matching_hello_is_answered_and_requests_and_events_flow() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let socket = dir.path().join("v.sock");
    let fake = FakeHelper::spawn(&socket, Some(hello("1a2b3c4d", &incarnation()?, 1, 1)))?;
    let Ok(client) = handshake(&socket, SHORT).await? else { bail!("the handshake failed") };
    let answer = client.request(RequestOp::Status(Status)).await?;
    ensure!(
        answer == ResponseResult::Status(StatusReply { exit: None, state: State::Idle.into() }),
        "the status answer is {answer:?}"
    );
    ensure!(client.event().await.is_some(), "no event arrived");
    let first = fake.received().into_iter().next().and_then(HostMessage::into_body);
    ensure!(first == Some(HostBody::Hello(HostHello { version: PROTOCOL_VERSION })), "the first message is {first:?}");
    Ok(())
}

/// The client refuses a helper for another workload and names the reason.
///
/// # Errors
///
/// Fails when the workload error or its refusal differs.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_helper_for_another_workload_is_refused_by_name() -> Result<()> {
    let (err, fake) = refused_by(Some(hello("deadbeef", &incarnation()?, 1, 1))).await?;
    ensure!(matches!(&err, HandshakeError::WrongWorkload(id) if id == "deadbeef"), "the error is {err}");
    expect_refusal(&fake, RefusalCode::WrongWorkload).await
}

/// The client reports a refusal that arrives in place of the hello, and says which one.
///
/// # Errors
///
/// Fails when the handshake doesn't end with the helper's refusal.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_refusal_in_place_of_the_hello_is_named() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let socket = dir.path().join("v.sock");
    let _fake = FakeHelper::spawn_refusing_peers(&socket, RefusalCode::WrongPeer, "the peer isn't avirthostd")?;
    let Err(err) = handshake(&socket, SHORT).await? else { bail!("the handshake succeeded") };
    ensure!(
        matches!(&err, HandshakeError::Refused { code: RefusalCode::WrongPeer, message } if message == "the peer isn't avirthostd"),
        "the error is {err}"
    );
    ensure!(
        err.to_string() == "avirtvmd refused the connection (wrong peer): the peer isn't avirthostd",
        "the message is {err}"
    );
    Ok(())
}

/// A helper that presents an old incarnation gets a named refusal.
///
/// # Errors
///
/// Fails when the incarnation error or its refusal differs.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_stale_incarnation_is_refused_by_name() -> Result<()> {
    let (err, fake) = refused_by(Some(hello("1a2b3c4d", &Incarnation::random(), 1, 1))).await?;
    ensure!(matches!(err, HandshakeError::StaleIncarnation), "the error is {err}");
    expect_refusal(&fake, RefusalCode::StaleIncarnation).await
}

/// A helper whose versions exclude the client's gets a named refusal.
///
/// # Errors
///
/// Fails when the version error or its refusal differs.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_version_range_without_ours_is_refused_by_name() -> Result<()> {
    let (err, fake) = refused_by(Some(hello("1a2b3c4d", &incarnation()?, 2, 2))).await?;
    ensure!(matches!(err, HandshakeError::UnsupportedVersion { max: 2, min: 2 }), "the error is {err}");
    expect_refusal(&fake, RefusalCode::UnsupportedVersion).await
}

/// A helper that never speaks times out.
///
/// # Errors
///
/// Fails when the handshake ends any other way.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_helper_that_sends_no_hello_times_out() -> Result<()> {
    let (err, _fake) = refused_by(None).await?;
    ensure!(matches!(err, HandshakeError::NoHello), "the error is {err}");
    Ok(())
}

/// A socket nobody listens on is a connect error.
///
/// # Errors
///
/// Fails when the handshake ends any other way.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_socket_nobody_listens_on_is_a_connect_error() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let Err(err) = handshake(&dir.path().join("v.sock"), SHORT).await? else { bail!("the handshake succeeded") };
    ensure!(matches!(err, HandshakeError::Connect(_)), "the error is {err}");
    Ok(())
}

/// A lost connection fails requests and ends the events.
///
/// # Errors
///
/// Fails when a request succeeds or an event arrives after the helper has gone.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_lost_connection_fails_requests_and_ends_the_events() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let socket = dir.path().join("v.sock");
    let fake = FakeHelper::spawn(&socket, Some(hello("1a2b3c4d", &incarnation()?, 1, 1)))?;
    let Ok(client) = handshake(&socket, SHORT).await? else { bail!("the handshake failed") };
    drop(fake);
    ensure!(client.event().await.is_none(), "an event arrived from a lost connection");
    ensure!(client.request(RequestOp::Status(Status)).await.is_err(), "a request succeeded on a lost connection");
    ensure!(client.is_closed(), "the client isn't closed");
    Ok(())
}

/// A frame over the cap ends the connection on its length alone.
///
/// # Errors
///
/// Fails when the handshake doesn't end with an I/O error.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_frame_over_the_cap_ends_the_connection() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let socket = dir.path().join("v.sock");
    let listener = UnixListener::bind(&socket)?;
    let sender = task::spawn(async move {
        let (mut stream, _) = listener.accept().await?;
        stream.write_all(&u32::try_from(MAX_FRAME.saturating_add(1))?.to_be_bytes()).await?;
        // Keep the connection open: avirthostd must give up on the length alone.
        time::sleep(Duration::from_secs(5)).await;
        anyhow::Ok(())
    });
    let Err(err) = handshake(&socket, Duration::from_secs(3)).await? else { bail!("the handshake succeeded") };
    sender.abort();
    ensure!(matches!(err, HandshakeError::Io(_)), "the error is {err}");
    Ok(())
}
