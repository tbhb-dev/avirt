//! The container commands and the wind-down of a finished workload.

use super::{StreamPhase, Supervised, Workload, lock, profile::Cleanup};
use core::time::Duration;
use serde_json::{Value, json};
use std::ffi::OsString;
use std::io;
use std::os::unix::process::ExitStatusExt as _;
use std::path::Path;
use std::process::{ExitStatus, Stdio};
use tokio::process::{Child, Command};
use tokio::signal::unix::Signal;
use tokio::task;
use tokio::time;
use tokio_util::sync::CancellationToken;

/// Resolves when either signal arrives.
pub(super) async fn first_signal(sigint: &mut Signal, sigterm: &mut Signal) {
    tokio::select! { _ = sigint.recv() => {}, _ = sigterm.recv() => {} }
}

/// Runs a `container` subcommand with a time limit. Returns true if it succeeded.
async fn container_cmd(args: &[&str], limit: Duration) -> bool {
    let run = Command::new("container")
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        // A separate process group prevents a second Ctrl-C from stopping it. Drop and stop it on timeout.
        .process_group(0)
        .kill_on_drop(true)
        .status();
    matches!(time::timeout(limit, run).await, Ok(Ok(status)) if status.success())
}

/// The name of a workload's container.
pub(super) fn container_name(id: &str) -> String {
    format!("avirt-{id}-workload")
}

/// The mount spec for a socket or directory: `host:guest`.
pub(super) fn mount(host: &Path, guest: &str) -> OsString {
    let mut spec = OsString::from(host);
    spec.push(":");
    spec.push(guest);
    spec
}

/// Runs the cleanup if the container no longer exists, and returns its note for the log.
pub(super) async fn run_cleanup(workload: &Workload, mut cleanup: Cleanup, gone: bool) -> Option<Value> {
    if gone {
        // Cleanup may block on a file lock.
        task::spawn_blocking(move || cleanup.run()).await.ok().flatten()
    } else {
        // A write-back could race an agent that's still running. Leave the temp
        // directory for the next startup's sweep.
        cleanup.abandon();
        workload.hold(json!({
            "event": "container_error",
            "message": "the container could not be confirmed gone; cleanup and write-back were skipped",
        }));
        None
    }
}

/// Gives a stream that's about to open, or still open, time to drain into the log.
pub(super) async fn settle_stream(workload: &Workload) {
    // While the mediator still splices, allow time to route a stream the container opened
    // just before dying (routing would otherwise refuse it with 410). Give an open stream
    // time to drain into the log.
    if lock(&workload.state).stream == StreamPhase::Unclaimed {
        time::sleep(Duration::from_secs(1)).await;
    }
    for _ in 0u8..200 {
        if lock(&workload.state).stream != StreamPhase::Streaming {
            break;
        }
        time::sleep(Duration::from_millis(50)).await;
    }
}

/// Stops the container and waits for the `container run` command-line interface to exit.
///
/// A stop that arrives before the container exists fails fast, so this function retries it at a
/// short interval. As a last resort, it kills and deletes the container by name before killing the
/// `container` process. The flag confirms whether the container no longer exists.
/// Without that confirmation, the caller must not change what the agent could still be using.
async fn stop_and_wait(id: &str, child: &mut Child) -> (io::Result<ExitStatus>, bool) {
    let name = container_name(id);
    for _ in 0u8..40 {
        let stopped = container_cmd(&["stop", "--time", "5", &name], Duration::from_secs(15)).await;
        let grace = if stopped { Duration::from_secs(10) } else { Duration::from_millis(300) };
        if let Ok(status) = time::timeout(grace, child.wait()).await {
            return (status, true);
        }
    }
    container_cmd(&["kill", &name], Duration::from_secs(10)).await;
    container_cmd(&["delete", "--force", &name], Duration::from_secs(10)).await;
    drop(child.start_kill());
    let status = time::timeout(Duration::from_secs(5), child.wait())
        .await
        .unwrap_or_else(|_| Err(io::Error::other("the container CLI did not exit")));
    // `inspect` fails once the container no longer exists.
    let gone = !container_cmd(&["inspect", &name], Duration::from_secs(10)).await;
    (status, gone)
}

/// Waits for the container to exit, stopping it if its mediator exits or avirthostd shuts down.
/// Returns the exit note and whether the container no longer exists.
pub(super) async fn watch_container(
    mut child: Child,
    run: &mut Supervised,
    shutdown: &CancellationToken,
) -> (io::Result<Value>, bool) {
    let mut gone = true;
    let status = tokio::select! {
        status = child.wait() => status,
        died = run.proxy.exited() => {
            // Without its mediator the container has no connection and no proxy.
            let how = died.map_or_else(|err| err.to_string(), |exit| exit.to_string());
            run.workload.hold(json!({
                "event": "container_error",
                "message": format!("the workload mediator exited unexpectedly ({how}); stopping the container"),
            }));
            let (status, confirmed) = stop_and_wait(&run.id, &mut child).await;
            gone = confirmed;
            status
        }
        () = shutdown.cancelled() => {
            let (status, confirmed) = stop_and_wait(&run.id, &mut child).await;
            gone = confirmed;
            status
        }
    };
    (status.map(|exit| json!({ "event": "container_exit", "code": exit.code(), "signal": exit.signal() })), gone)
}
