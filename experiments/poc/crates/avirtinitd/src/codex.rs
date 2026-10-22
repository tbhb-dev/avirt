//! The Codex workload: an ACP v1 client for `codex-acp`.

use std::io::{Write as _, stdout};
use std::process::ExitCode;

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v1::{
    ClientCapabilities, ContentBlock, Implementation, InitializeRequest, NewSessionRequest, PermissionOption,
    PermissionOptionKind, PromptRequest, RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SelectedPermissionOutcome, SessionNotification, SessionUpdate,
};
use agent_client_protocol::{Agent, Client, ConnectionTo, Error, Responder};
use avirt_core::connection::LogStream;
use avirt_core::console;
use avirt_core::harness::{self, Spec, Workload, pick_option};
use serde_json::json;

/// The name the adapter reports in its events.
const NAME: &str = "codex-container";

/// Runs the Codex adapter through the shared harness.
pub(crate) async fn run() -> ExitCode {
    // The adapter inherits CODEX_HOME, which points at the workload's mounted credentials.
    harness::run(Spec { agent: "codex", agent_command: "codex-acp", agent_env: &[] }, converse).await
}

/// Answers a permission request with the first allow option, or cancels if there is none.
fn approve(connection: &LogStream, options: &[PermissionOption]) -> RequestPermissionResponse {
    let kind = |option: &PermissionOption| {
        if matches!(option.kind, PermissionOptionKind::AllowOnce) {
            "allow_once"
        } else if matches!(option.kind, PermissionOptionKind::AllowAlways) {
            "allow_always"
        } else {
            "other"
        }
    };
    let Some(choice) = pick_option(options, kind) else {
        return RequestPermissionResponse::new(RequestPermissionOutcome::Cancelled);
    };
    connection.meta(json!({ "event": "auto_approve", "optionId": choice.option_id, "name": choice.name }));
    RequestPermissionResponse::new(RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
        choice.option_id.clone(),
    )))
}

/// Prints agent text as it streams and the name of every other update.
fn show(update: &SessionUpdate) {
    if let SessionUpdate::AgentMessageChunk(chunk) = update {
        if let ContentBlock::Text(text) = &chunk.content {
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

/// Runs one prompt against the Codex adapter and returns the stop reason.
///
/// # Errors
///
/// Returns the adapter's error, or an invalid-params error if it doesn't speak ACP v1.
async fn converse(workload: Workload) -> Result<String, Box<Error>> {
    let Workload { agent, connection, prompt, cwd } = workload;
    let permission_connection = connection.clone();
    let init_connection = connection.clone();

    // A handler shows updates as they arrive, independently of the prompt request. It doesn't wait
    // for the turn to end or discard output if the adapter rejects the prompt. In v1 the `session/prompt`
    // response is the end of the turn and includes the stop reason.
    Client
        .builder()
        .name(NAME)
        .on_receive_notification(
            async move |notification: SessionNotification, _cx: ConnectionTo<Agent>| {
                show(&notification.update);
                Ok(())
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .on_receive_request(
            async move |req: RequestPermissionRequest, responder: Responder<RequestPermissionResponse>, _cx| {
                responder.respond(approve(&permission_connection, &req.options))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_with(agent, async move |cx: ConnectionTo<Agent>| {
            // codex-acp speaks ACP v1 only.
            let init = cx
                .send_request(
                    InitializeRequest::new(ProtocolVersion::V1)
                        .client_capabilities(ClientCapabilities::new())
                        .client_info(Implementation::new(NAME, "0.0.0")),
                )
                .block_task()
                .await?;
            if init.protocol_version != ProtocolVersion::V1 {
                return Err(Error::invalid_params()
                    .data(format!("agent answered protocol version {:?}, expected 1", init.protocol_version)));
            }
            let ids: Vec<_> = init.auth_methods.iter().map(|method| method.id().to_string()).collect();
            init_connection.meta(json!({ "event": "auth_methods", "ids": ids }));
            // No authenticate call: Codex reads the mounted auth.json itself.
            let created = cx.send_request(NewSessionRequest::new(cwd)).block_task().await?;
            let done =
                cx.send_request(PromptRequest::new(created.session_id, vec![prompt.into()])).block_task().await?;
            let reason = serde_json::to_value(done.stop_reason).unwrap_or_default();
            Ok(reason.as_str().unwrap_or("unknown").to_owned())
        })
        .await
        .map_err(Box::new)
}
