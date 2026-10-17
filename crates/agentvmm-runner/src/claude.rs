use std::io::Write;
use std::process::ExitCode;

use agent_client_protocol::schema::{ProtocolVersion, v2};
use agent_client_protocol::{Agent, Client, Error, Responder, V2ConnectionTo};
use agentvmm_core::harness::{self, Actor, Spec, pick_option};
use agentvmm_core::uplink::Uplink;
use serde_json::{Value, json};
use tokio::sync::mpsc::unbounded_channel;

const NAME: &str = "claude-container";

pub async fn run() -> ExitCode {
    // ACP v2 is a draft; the adapter only speaks it behind this flag.
    let spec = Spec {
        agent: "claude",
        agent_command: "claude-agent-acp",
        agent_env: &[("CLAUDE_AGENT_ACP_EXPERIMENTAL_V2", "1")],
    };
    harness::run(spec, converse).await
}

fn approve(uplink: &Uplink, options: &[v2::PermissionOption]) -> v2::RequestPermissionResponse {
    let kind = |o: &v2::PermissionOption| match &o.kind {
        v2::PermissionOptionKind::AllowOnce => "allow_once",
        v2::PermissionOptionKind::AllowAlways => "allow_always",
        _ => "other",
    };
    let Some(choice) = pick_option(options, kind) else {
        return v2::RequestPermissionResponse::new(v2::RequestPermissionOutcome::Cancelled);
    };
    uplink.meta(json!({ "event": "auto_approve", "optionId": choice.option_id, "name": choice.name }));
    v2::RequestPermissionResponse::new(v2::RequestPermissionOutcome::Selected(v2::SelectedPermissionOutcome::new(
        choice.option_id.clone(),
    )))
}

/// Prints agent text as it streams and the name of every other update.
fn show(update: &v2::SessionUpdate) {
    match update {
        v2::SessionUpdate::AgentMessageChunk(chunk) => match &chunk.content {
            v2::ContentBlock::Text(text) => {
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
    let (updates_tx, mut updates) = unbounded_channel::<v2::UpdateSessionNotification>();
    let permission_uplink = uplink.clone();

    Client
        .v2()
        .name(NAME)
        .on_receive_notification(
            async move |n: v2::UpdateSessionNotification, _cx: V2ConnectionTo<Agent>| {
                updates_tx.send(n).map_err(Error::into_internal_error)
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .on_receive_request(
            async move |req: v2::RequestPermissionRequest,
                        responder: Responder<v2::RequestPermissionResponse>,
                        _cx: V2ConnectionTo<Agent>| {
                responder.respond(approve(&permission_uplink, &req.options))
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
            // Whatever queued before the prompt (session setup updates) is not part of the turn.
            while updates.try_recv().is_ok() {}
            // The response only acknowledges the prompt; updates arrive independently.
            session.send_prompt(&prompt).block_task().await?;
            let mut active = false;
            loop {
                let n = updates.recv().await.ok_or_else(|| {
                    Error::internal_error().data("agent disconnected before the prompt ran to completion")
                })?;
                if &n.session_id != session.session_id() {
                    continue;
                }
                match &n.update {
                    v2::SessionUpdate::StateUpdate(v2::StateUpdate::Idle(idle)) => {
                        // The stop reason is an internally tagged enum, so its name is under
                        // `stopReason`.
                        let reason = serde_json::to_value(&idle.stop_reason).unwrap_or(Value::Null);
                        // An idle that ends foreground work carries a stop reason even if the
                        // session never reported running (immediate refusal); one without is just
                        // the ready state and counts only after the session was active.
                        if active || reason["stopReason"].is_string() {
                            return Ok(reason["stopReason"].as_str().unwrap_or("idle").to_owned());
                        }
                    }
                    v2::SessionUpdate::StateUpdate(
                        v2::StateUpdate::Running(_) | v2::StateUpdate::RequiresAction(_),
                    ) => {
                        active = true;
                    }
                    _ => {}
                }
                show(&n.update);
            }
        })
        .await
}
