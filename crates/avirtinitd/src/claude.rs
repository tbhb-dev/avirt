//! The Claude workload: an ACP v2 client for `claude-agent-acp`.

use std::io::{Write as _, stdout};
use std::process::ExitCode;

use agent_client_protocol::schema::{ProtocolVersion, v2};
use agent_client_protocol::{Agent, Client, Error, Responder, V2ConnectionTo};
use avirt_core::connection::LogStream;
use avirt_core::console;
use avirt_core::harness::{self, Spec, Workload, pick_option};
use serde_json::{Value, json};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

/// The name the adapter reports in its events.
const NAME: &str = "claude-container";

/// Runs the Claude adapter through the shared harness.
pub(crate) async fn run() -> ExitCode {
    // ACP v2 is a draft. The adapter only speaks it behind this flag.
    let spec = Spec {
        agent: "claude",
        agent_command: "claude-agent-acp",
        agent_env: &[("CLAUDE_AGENT_ACP_EXPERIMENTAL_V2", "1")],
    };
    harness::run(spec, converse).await
}

/// Answers a permission request with the first allow option, or cancels if there is none.
fn approve(connection: &LogStream, options: &[v2::PermissionOption]) -> v2::RequestPermissionResponse {
    let kind = |option: &v2::PermissionOption| {
        if matches!(option.kind, v2::PermissionOptionKind::AllowOnce) {
            "allow_once"
        } else if matches!(option.kind, v2::PermissionOptionKind::AllowAlways) {
            "allow_always"
        } else {
            "other"
        }
    };
    let Some(choice) = pick_option(options, kind) else {
        return v2::RequestPermissionResponse::new(v2::RequestPermissionOutcome::Cancelled);
    };
    connection.meta(json!({ "event": "auto_approve", "optionId": choice.option_id, "name": choice.name }));
    v2::RequestPermissionResponse::new(v2::RequestPermissionOutcome::Selected(v2::SelectedPermissionOutcome::new(
        choice.option_id.clone(),
    )))
}

/// Prints agent text as it streams and the name of every other update.
fn show(update: &v2::SessionUpdate) {
    if let v2::SessionUpdate::AgentMessageChunk(chunk) = update {
        if let v2::ContentBlock::Text(text) = &chunk.content {
            console::out(format_args!("{}", text.text));
            let _flushed = stdout().flush();
        } else {
            console::out_line(format_args!("[agent_message_chunk]"));
        }
    } else {
        let name =
            serde_json::to_value(update).ok().and_then(|value| value.get("sessionUpdate")?.as_str().map(str::to_owned));
        console::out_line(format_args!("[{}]", name.unwrap_or_default()));
    }
}

/// Marks the turn active when the session starts running or needs input.
///
/// Returns the stop reason once the session goes idle after the turn.
fn turn_outcome(update: &v2::SessionUpdate, active: &mut bool) -> Option<String> {
    let v2::SessionUpdate::StateUpdate(state) = update else {
        return None;
    };
    if matches!(state, v2::StateUpdate::Running(_) | v2::StateUpdate::RequiresAction(_)) {
        *active = true;
        return None;
    }
    let v2::StateUpdate::Idle(idle) = state else {
        return None;
    };
    // The stop reason is an internally tagged enum, so its name is under `stopReason`.
    let reason = serde_json::to_value(&idle.stop_reason).unwrap_or(Value::Null);
    let stop = reason.get("stopReason").and_then(Value::as_str);
    // An idle that ends foreground work includes a stop reason even if the session never
    // reported running (immediate refusal). An idle without a stop reason is the ready state
    // and counts only after the session was active.
    (*active || stop.is_some()).then(|| stop.unwrap_or("idle").to_owned())
}

/// Shows updates for the session until it goes idle, and returns the stop reason.
///
/// # Errors
///
/// Returns an internal error if the agent disconnects before the turn ends.
async fn await_turn(
    updates: &mut UnboundedReceiver<v2::UpdateSessionNotification>,
    session_id: &v2::SessionId,
) -> Result<String, Box<Error>> {
    let mut active = false;
    loop {
        let notification = updates.recv().await.ok_or_else(|| {
            Box::new(Error::internal_error().data("agent disconnected before the prompt ran to completion"))
        })?;
        if &notification.session_id != session_id {
            continue;
        }
        if let Some(reason) = turn_outcome(&notification.update, &mut active) {
            return Ok(reason);
        }
        show(&notification.update);
    }
}

/// Runs one prompt against the Claude adapter and returns the stop reason.
///
/// # Errors
///
/// Returns the adapter's error, or an invalid-params error if it doesn't speak ACP v2.
async fn converse(workload: Workload) -> Result<String, Box<Error>> {
    let Workload { agent, connection, prompt, cwd } = workload;
    let (updates_tx, mut updates) = unbounded_channel::<v2::UpdateSessionNotification>();
    let permission_connection = connection.clone();

    Client
        .v2()
        .name(NAME)
        .on_receive_notification(
            async move |notification: v2::UpdateSessionNotification, _cx: V2ConnectionTo<Agent>| {
                updates_tx.send(notification).map_err(Error::into_internal_error)
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .on_receive_request(
            async move |req: v2::RequestPermissionRequest,
                        responder: Responder<v2::RequestPermissionResponse>,
                        _cx: V2ConnectionTo<Agent>| {
                responder.respond(approve(&permission_connection, &req.options))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_with(agent, async move |cx| {
            let init = cx
                .send_request(v2::InitializeRequest::new(ProtocolVersion::V2, v2::Implementation::new(NAME, "0.0.0")))
                .block_task()
                .await?;
            if init.protocol_version != ProtocolVersion::V2 {
                return Err(Error::invalid_params()
                    .data(format!("agent answered protocol version {:?}, expected 2", init.protocol_version)));
            }
            let session = cx.build_session(&cwd).start_session().block_task().await?.into_session();
            // Whatever queued before the prompt (session setup updates) isn't part of the turn.
            while updates.try_recv().is_ok() {}
            // The response only acknowledges the prompt. Updates arrive independently.
            session.send_prompt(&prompt).block_task().await?;
            await_turn(&mut updates, session.session_id()).await.map_err(|err| *err)
        })
        .await
        .map_err(Box::new)
}
