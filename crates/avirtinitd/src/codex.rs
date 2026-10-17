use std::io::Write;
use std::process::ExitCode;

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v1::{
    ClientCapabilities, ContentBlock, Implementation, InitializeRequest, NewSessionRequest, PermissionOption,
    PermissionOptionKind, PromptRequest, RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SelectedPermissionOutcome, SessionNotification, SessionUpdate,
};
use agent_client_protocol::{Agent, Client, ConnectionTo, Error, Responder};
use avirt_core::harness::{self, Actor, Spec, pick_option};
use avirt_core::uplink::Uplink;
use serde_json::json;

const NAME: &str = "codex-container";

pub async fn run() -> ExitCode {
    // CODEX_HOME points at the actor's mounted credentials and is inherited by the adapter.
    harness::run(Spec { agent: "codex", agent_command: "codex-acp", agent_env: &[] }, converse).await
}

fn approve(uplink: &Uplink, options: &[PermissionOption]) -> RequestPermissionResponse {
    let kind = |o: &PermissionOption| match o.kind {
        PermissionOptionKind::AllowOnce => "allow_once",
        PermissionOptionKind::AllowAlways => "allow_always",
        _ => "other",
    };
    let Some(choice) = pick_option(options, kind) else {
        return RequestPermissionResponse::new(RequestPermissionOutcome::Cancelled);
    };
    uplink.meta(json!({ "event": "auto_approve", "optionId": choice.option_id, "name": choice.name }));
    RequestPermissionResponse::new(RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
        choice.option_id.clone(),
    )))
}

/// Prints agent text as it streams and the name of every other update.
fn show(update: &SessionUpdate) {
    match update {
        SessionUpdate::AgentMessageChunk(chunk) => match &chunk.content {
            ContentBlock::Text(text) => {
                print!("{}", text.text);
                let _ = std::io::stdout().flush();
            }
            _ => println!("[agent_message_chunk]"),
        },
        other => {
            let name = serde_json::to_value(other).ok().and_then(|v| v["sessionUpdate"].as_str().map(str::to_owned));
            println!("[{}]", name.unwrap_or_default());
        }
    }
}

async fn converse(actor: Actor) -> Result<String, Error> {
    let Actor { agent, uplink, prompt, cwd } = actor;
    let permission_uplink = uplink.clone();
    let init_uplink = uplink.clone();

    // Updates are shown by a handler as they arrive, independently of the prompt request, so
    // output is not delayed until the turn ends and survives a rejected prompt. In v1 the
    // `session/prompt` response is the end of the turn and carries the stop reason.
    Client
        .builder()
        .name(NAME)
        .on_receive_notification(
            async move |n: SessionNotification, _cx: ConnectionTo<Agent>| {
                show(&n.update);
                Ok(())
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .on_receive_request(
            async move |req: RequestPermissionRequest, responder: Responder<RequestPermissionResponse>, _cx| {
                responder.respond(approve(&permission_uplink, &req.options))
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
            let ids: Vec<_> = init.auth_methods.iter().map(|m| m.id().to_string()).collect();
            init_uplink.meta(json!({ "event": "auth_methods", "ids": ids }));
            // No authenticate call: Codex reads the mounted auth.json itself.
            let created = cx.send_request(NewSessionRequest::new(cwd)).block_task().await?;
            let done =
                cx.send_request(PromptRequest::new(created.session_id, vec![prompt.into()])).block_task().await?;
            let reason = serde_json::to_value(done.stop_reason).unwrap_or_default();
            Ok(reason.as_str().unwrap_or("unknown").to_owned())
        })
        .await
}
