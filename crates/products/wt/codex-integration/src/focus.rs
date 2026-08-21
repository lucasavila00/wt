use anyhow::{bail, Context, Result};
use std::process::Command;
use uuid::Uuid;
use wt_agent_tool_gateway::{
    valid_codex_pane_id, valid_codex_tmux_session, CODEX_SESSION_PANE_OPTION,
};

pub(crate) fn focus(session_id: Uuid, tmux_session: &str, pane_id: &str) -> Result<String> {
    if !valid_codex_tmux_session(tmux_session) {
        bail!("invalid Codex tmux session: {tmux_session}");
    }
    if !valid_codex_pane_id(pane_id) {
        bail!("invalid Codex pane ID: {pane_id}");
    }

    let expected = format!("{tmux_session}:{pane_id}:{session_id}:0");
    let output = Command::new("/usr/bin/tmux")
        .args([
            "display-message",
            "-p",
            "-t",
            pane_id,
            &format!(
                "#{{session_name}}:#{{pane_id}}:#{{{CODEX_SESSION_PANE_OPTION}}}:#{{pane_dead}}"
            ),
        ])
        .output()
        .context("inspect Codex Byobu target")?;
    let actual = format!("{expected}\n");
    if !output.status.success() || output.stdout != actual.as_bytes() {
        bail!(
            "Codex Byobu target mismatch: expected {expected}, received {}",
            escaped(&output.stdout)
        );
    }

    let output = Command::new("/usr/bin/tmux")
        .args([
            "select-window",
            "-t",
            pane_id,
            ";",
            "select-pane",
            "-t",
            pane_id,
        ])
        .output()
        .context("focus Codex Byobu target")?;
    if !output.status.success() || !output.stdout.is_empty() {
        bail!(
            "could not focus Codex Byobu target: status {}; stdout {}; stderr {}",
            output.status,
            escaped(&output.stdout),
            escaped(&output.stderr)
        );
    }
    Ok(expected)
}

fn escaped(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).escape_default().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_targets_before_running_tmux() {
        let session_id = Uuid::nil();
        insta::assert_snapshot!(
            focus(session_id, "other", "%1").unwrap_err(),
            @"invalid Codex tmux session: other"
        );
        insta::assert_snapshot!(
            focus(session_id, "wt-app", "%bad").unwrap_err(),
            @"invalid Codex pane ID: %bad"
        );
    }
}
