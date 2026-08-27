use anyhow::{bail, Context, Result};
use std::process::Command;
use wt_agent_tool_gateway::{valid_byobu_pane_id, valid_byobu_tmux_session};

pub(crate) fn focus(tmux_session: &str, pane_id: &str) -> Result<String> {
    let expected = inspect(tmux_session, pane_id)?;

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
        .context("focus Codex Byobu pane")?;
    if !output.status.success() || !output.stdout.is_empty() {
        bail!(
            "could not focus Codex Byobu pane: status {}; stdout {}; stderr {}",
            output.status,
            escaped(&output.stdout),
            escaped(&output.stderr)
        );
    }
    Ok(expected)
}

fn inspect(tmux_session: &str, pane_id: &str) -> Result<String> {
    if !valid_byobu_tmux_session(tmux_session) {
        bail!("invalid Byobu tmux session: {tmux_session}");
    }
    if !valid_byobu_pane_id(pane_id) {
        bail!("invalid Byobu pane ID: {pane_id}");
    }

    let expected = format!("{tmux_session}:{pane_id}:codex:0");
    let output = Command::new("/usr/bin/tmux")
        .args([
            "display-message",
            "-p",
            "-t",
            pane_id,
            "#{session_name}:#{pane_id}:#{pane_current_command}:#{pane_dead}",
        ])
        .output()
        .context("inspect Codex Byobu pane")?;
    let actual = format!("{expected}\n");
    if !output.status.success() || output.stdout != actual.as_bytes() {
        bail!(
            "Codex Byobu pane mismatch: status {}; expected stdout {}; actual stdout {}; stderr {}",
            output.status,
            escaped(actual.as_bytes()),
            escaped(&output.stdout),
            escaped(&output.stderr)
        );
    }

    Ok(format!("{tmux_session}:{pane_id}"))
}

fn escaped(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).escape_default().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_targets_before_running_tmux() {
        insta::assert_snapshot!(
            focus("other", "%1").unwrap_err(),
            @"invalid Byobu tmux session: other"
        );
        insta::assert_snapshot!(
            focus("wt-host", "%bad").unwrap_err(),
            @"invalid Byobu pane ID: %bad"
        );
    }
}
