//! The wind-down of a finished workload, from its exit note to its cleanup and its stream.

use super::{StreamPhase, Supervised, Workload, lock, profile::Cleanup};
use crate::runtime::{Exit, WorkloadExit};
use core::time::Duration;
use serde_json::{Value, json};
use std::io;
use tokio::signal::unix::Signal;
use tokio::task;
use tokio::time;
use tokio_util::sync::CancellationToken;

/// How long a runtime lets a container stop on its own before it forces one.
const STOP_GRACE: Duration = Duration::from_secs(5);

/// Resolves when either signal arrives.
pub(super) async fn first_signal(sigint: &mut Signal, sigterm: &mut Signal) {
    tokio::select! { _ = sigint.recv() => {}, _ = sigterm.recv() => {} }
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

/// The `container_exit` note for a workload that ended.
///
/// # Errors
///
/// Passes on the error that stopped the runtime from reporting an exit.
fn exit_note(status: io::Result<WorkloadExit>) -> io::Result<Value> {
    status.map(|exit| json!({ "event": "container_exit", "code": exit.code, "signal": exit.signal }))
}

/// Waits for the workload to end, stopping it if its mediator exits, the runtime loses it, or
/// avirthostd shuts down. Returns the exit note and whether the container no longer exists.
pub(super) async fn watch_container(run: &mut Supervised, shutdown: &CancellationToken) -> (io::Result<Value>, bool) {
    let stopped = tokio::select! {
        exit = run.handle.wait() => match exit {
            Exit::Workload(status) => return (exit_note(status), true),
            Exit::Mediator(how) => {
                // Without its mediator the container has no connection and no proxy.
                run.workload.hold(json!({
                    "event": "container_error",
                    "message": format!("the workload mediator exited unexpectedly ({how}); stopping the container"),
                }));
                run.handle.stop(STOP_GRACE).await
            }
            Exit::Lost(why) => {
                run.workload.hold(json!({ "event": "container_error", "message": why }));
                run.handle.stop(STOP_GRACE).await
            }
        },
        () = shutdown.cancelled() => run.handle.stop(STOP_GRACE).await,
    };
    (exit_note(stopped.exit), stopped.gone)
}
