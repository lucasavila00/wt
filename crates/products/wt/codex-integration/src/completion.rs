use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;
use wt_agent_tool_gateway::{
    read_json_line, write_json_line, ClientOperation, ClientRequest, CodexTurnStatus,
    TransportResponse, PROTOCOL_VERSION,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Completion {
    pub thread_id: String,
    pub turn_id: String,
    pub pane_id: Option<String>,
    pub status: CodexTurnStatus,
    pub message: String,
}

pub(crate) fn from_turn(
    thread_id: &str,
    pane_id: Option<&str>,
    turn: &Value,
) -> Result<Completion> {
    let turn_id = turn
        .get("id")
        .and_then(Value::as_str)
        .context("turn has no ID")?;
    let (status, message) = match turn.get("status").and_then(Value::as_str) {
        Some("completed") => (
            CodexTurnStatus::Completed,
            turn.get("items")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .rev()
                .find(|item| item.get("type").and_then(Value::as_str) == Some("agentMessage"))
                .and_then(|item| item.get("text"))
                .and_then(Value::as_str)
                .unwrap_or("Codex turn completed")
                .to_owned(),
        ),
        Some("failed" | "interrupted") => (
            CodexTurnStatus::Failed,
            turn.pointer("/error/message")
                .and_then(Value::as_str)
                .unwrap_or("Codex turn failed")
                .to_owned(),
        ),
        other => bail!("Codex turn is not terminal: {other:?}"),
    };
    Ok(Completion {
        thread_id: thread_id.into(),
        turn_id: turn_id.into(),
        pane_id: pane_id.map(str::to_owned),
        status,
        message,
    })
}

pub(crate) fn deliver(completion: &Completion, socket: &Path) -> Result<()> {
    let mut relay = UnixStream::connect(socket).context("connect to WT guest relay")?;
    relay.set_read_timeout(Some(Duration::from_secs(30)))?;
    relay.set_write_timeout(Some(Duration::from_secs(30)))?;
    write_json_line(
        &mut relay,
        &ClientRequest {
            protocol_version: PROTOCOL_VERSION,
            operation: ClientOperation::CodexTurnFinished {
                thread_id: completion.thread_id.clone(),
                turn_id: completion.turn_id.clone(),
                pane_id: completion.pane_id.clone(),
                status: completion.status,
                message: completion.message.clone(),
            },
        },
    )
    .context("send Codex completion to WT guest relay")?;
    let response: TransportResponse =
        read_json_line(&mut relay).context("read Codex completion response")?;
    if !response.ok {
        bail!(
            "WT rejected Codex completion: {}",
            response.error.unwrap_or_default()
        );
    }
    Ok(())
}
