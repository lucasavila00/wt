use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::env;
use std::io;
use std::os::unix::net::UnixStream;
use std::process::Command;
use std::time::Duration;
use uuid::Uuid;
use wt_agent_tool_gateway::{
    read_json_line, write_json_line, ClientOperation, ClientRequest, CodexSessionEvent,
    CodexSessionEventKind, TransportResponse, PROTOCOL_VERSION, RELAY_SOCKET,
};

const TIMEOUT: Duration = Duration::from_millis(250);

#[derive(Debug, Deserialize)]
struct HookPayload {
    session_id: Uuid,
    cwd: String,
    hook_event_name: HookEventName,
}

#[derive(Clone, Copy, Debug, Deserialize)]
enum HookEventName {
    SessionStart,
    UserPromptSubmit,
    Stop,
    SessionEnd,
}

pub(crate) fn report_hook() -> Result<()> {
    let payload: HookPayload = serde_json::from_reader(io::stdin()).context("decode Codex hook")?;
    let pane_id = env::var("WT_BYOBU_PANE")
        .or_else(|_| env::var("TMUX_PANE"))
        .context("Codex is not running in a WT Byobu pane")?;
    let tmux_session = match env::var("WT_BYOBU_SESSION") {
        Ok(value) => value,
        Err(_) => current_tmux_session(&pane_id)?,
    };
    let event = CodexSessionEvent {
        session_id: payload.session_id,
        cwd: payload.cwd,
        tmux_session,
        pane_id,
        kind: match payload.hook_event_name {
            HookEventName::SessionStart => CodexSessionEventKind::SessionStart,
            HookEventName::UserPromptSubmit => CodexSessionEventKind::UserPromptSubmit,
            HookEventName::Stop => CodexSessionEventKind::Stop,
            HookEventName::SessionEnd => CodexSessionEventKind::SessionEnd,
        },
    };
    let mut stream = UnixStream::connect(RELAY_SOCKET).context("connect to WT guest relay")?;
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))?;
    write_json_line(
        &mut stream,
        &ClientRequest {
            protocol_version: PROTOCOL_VERSION,
            operation: ClientOperation::CodexSession { event },
        },
    )?;
    let response: TransportResponse = read_json_line(&mut stream)?;
    if !response.ok {
        bail!(
            "WT guest relay rejected Codex session report: {}",
            response.error.as_deref().unwrap_or("unknown error")
        );
    }
    Ok(())
}

fn current_tmux_session(pane_id: &str) -> Result<String> {
    let output = Command::new("/usr/bin/tmux")
        .args(["display-message", "-p", "-t", pane_id, "#{session_name}"])
        .output()
        .context("resolve current Byobu session")?;
    if !output.status.success() {
        bail!("Codex Byobu pane is not active");
    }
    let session = String::from_utf8(output.stdout).context("decode Byobu session name")?;
    Ok(session.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_supported_hook_payloads() {
        let payload: HookPayload = serde_json::from_str(
            r#"{"session_id":"123e4567-e89b-12d3-a456-426614174000","cwd":"/workspace","hook_event_name":"Stop","extra":"ignored"}"#,
        )
        .unwrap();

        assert_eq!(
            payload.session_id,
            Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").unwrap()
        );
        assert!(matches!(payload.hook_event_name, HookEventName::Stop));
    }
}
