use super::Registration;
use anyhow::{bail, Context, Result};
use std::process::Command;
use wt_agent_tool_gateway::{CodexSessionEvent, CodexSessionEventKind};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PaneLiveness {
    Running,
    Closed,
    Missing,
}

pub(super) fn codex_pane_liveness(
    session_id: uuid::Uuid,
    registration: &Registration,
) -> PaneLiveness {
    let output = Command::new("/usr/bin/tmux")
        .args([
            "display-message",
            "-p",
            "-t",
            &registration.pane_id,
            "#{session_name}:#{pane_id}:#{@wt_codex_session_id}:#{pane_current_command}",
        ])
        .output();
    let Ok(output) = output else {
        return PaneLiveness::Missing;
    };
    if !output.status.success() {
        return PaneLiveness::Missing;
    }
    parse_codex_pane_liveness(session_id, registration, &output.stdout)
}

fn parse_codex_pane_liveness(
    session_id: uuid::Uuid,
    registration: &Registration,
    output: &[u8],
) -> PaneLiveness {
    let prefix = format!(
        "{}:{}:{}:",
        registration.tmux_session, registration.pane_id, session_id
    );
    let Some(command) = output
        .strip_prefix(prefix.as_bytes())
        .and_then(|command| command.strip_suffix(b"\n"))
    else {
        return PaneLiveness::Missing;
    };
    if command == b"codex" {
        PaneLiveness::Running
    } else {
        PaneLiveness::Closed
    }
}

pub(super) fn infer_session_end(
    session_id: uuid::Uuid,
    registration: &Registration,
) -> Result<CodexSessionEvent> {
    if registration.pane_sequence == 0 {
        bail!("Codex session has no tracked event order");
    }
    let pane_sequence = registration
        .pane_sequence
        .checked_add(1)
        .context("Codex pane event sequence overflow")?;
    Ok(CodexSessionEvent {
        session_id,
        cwd: registration.cwd.clone(),
        tmux_session: registration.tmux_session.clone(),
        pane_id: registration.pane_id.clone(),
        kind: CodexSessionEventKind::SessionEnd,
        pane_generation: registration.pane_generation,
        pane_sequence,
        session_start_source: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registration() -> Registration {
        Registration {
            cwd: "/home/wt/project".into(),
            tmux_session: "wt-host".into(),
            pane_id: "%1".into(),
            pane_generation: 3,
            pane_sequence: 7,
        }
    }

    #[test]
    fn detects_when_a_marked_codex_pane_returns_to_the_shell() {
        let session_id = uuid::Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").unwrap();
        let registration = registration();

        assert_eq!(
            parse_codex_pane_liveness(
                session_id,
                &registration,
                b"wt-host:%1:123e4567-e89b-12d3-a456-426614174000:codex\n",
            ),
            PaneLiveness::Running
        );
        assert_eq!(
            parse_codex_pane_liveness(
                session_id,
                &registration,
                b"wt-host:%1:123e4567-e89b-12d3-a456-426614174000:bash\n",
            ),
            PaneLiveness::Closed
        );
    }

    #[test]
    fn closed_codex_pane_gets_the_next_ordered_session_end() {
        let session_id = uuid::Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").unwrap();
        let event = infer_session_end(session_id, &registration()).unwrap();

        assert_eq!(event.kind, CodexSessionEventKind::SessionEnd);
        assert_eq!(event.pane_generation, 3);
        assert_eq!(event.pane_sequence, 8);
        assert_eq!(event.tmux_session, "wt-host");
        assert_eq!(event.pane_id, "%1");
    }
}
