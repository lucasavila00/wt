use anyhow::{bail, Context, Result};
use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

const HELP: &str = r"WT guest runtime

USAGE:
    wtg <COMMAND>

COMMANDS:
    tools    Run a provider command; use `wtg tools --help` for details
    help     Print this help message
";

fn main() {
    let args = std::env::args_os().collect::<Vec<_>>();
    let tools = args.get(1).is_some_and(|arg| arg == "tools");
    let plain_tools = tools
        && args.get(2).is_some_and(|arg| {
            matches!(
                arg.to_str(),
                Some("help" | "--help" | "-h" | "world-prompt")
            )
        });
    if let Err(error) = run(args) {
        if tools && !plain_tools {
            eprintln!(
                "{}",
                wt_agent_tool_gateway::tools_command::render_error(&format!("{error:#}"))
            );
        } else {
            eprintln!("wtg: {error:#}");
        }
        std::process::exit(1);
    }
}

fn run(mut args: Vec<OsString>) -> Result<()> {
    if invoked_as(&args, "git-remote-wt-agent") {
        return wt_agent_tool_gateway::git_remote_command::run_from(utf8_args(args.drain(1..))?);
    }
    let command = args.get(1).and_then(|arg| arg.to_str());
    match command {
        Some("help" | "--help" | "-h") => {
            if args.len() != 2 {
                bail!("usage: wtg help");
            }
            print!("{HELP}");
            Ok(())
        }
        Some("relay") => {
            args.remove(1);
            args[0] = "wtg relay".into();
            wt_agent_tool_gateway::relay_command::run_from(args)
        }
        Some("codex") => run_codex(&args[2..]),
        Some("tools") => wt_agent_tool_gateway::tools_command::run(utf8_args(args.drain(2..))?),
        Some(other) => bail!("unknown guest command {other:?}; run `wtg help` for usage"),
        None => bail!("missing guest command; run `wtg help` for usage"),
    }
}

fn run_codex(args: &[OsString]) -> Result<()> {
    match args {
        [command, tmux_session, pane_id] if command == "focus-pane" => {
            println!("{}", focus_pane(os_str(tmux_session)?, os_str(pane_id)?)?);
            Ok(())
        }
        [command, ..] if command == "focus-pane" => {
            bail!("usage: wtg codex focus-pane <TMUX_SESSION> <PANE_ID>")
        }
        [command, ..] => bail!("unknown Codex command {command:?}"),
        [] => bail!("missing Codex command; expected focus-pane"),
    }
}

fn focus_pane(tmux_session: &str, pane_id: &str) -> Result<String> {
    if !wt_agent_tool_gateway::valid_byobu_tmux_session(tmux_session) {
        bail!("invalid Byobu tmux session: {tmux_session}");
    }
    if !wt_agent_tool_gateway::valid_byobu_pane_id(pane_id) {
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

    Ok(format!("{tmux_session}:{pane_id}"))
}

fn os_str(value: &OsString) -> Result<&str> {
    value
        .to_str()
        .context("Codex focus-pane arguments must be UTF-8")
}

fn escaped(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).escape_default().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_level_help_lists_public_guest_commands() {
        insta::assert_snapshot!(HELP, @r#"
        WT guest runtime

        USAGE:
            wtg <COMMAND>

        COMMANDS:
            tools    Run a provider command; use `wtg tools --help` for details
            help     Print this help message
        "#);
    }

    #[test]
    fn missing_command_points_to_help() {
        let error = run(vec!["wtg".into()]).unwrap_err();

        assert_eq!(
            error.to_string(),
            "missing guest command; run `wtg help` for usage"
        );
    }

    #[test]
    fn codex_focus_pane_reaches_the_guest_focus_command() {
        let error = run(vec![
            "wtg".into(),
            "codex".into(),
            "focus-pane".into(),
            "other".into(),
            "%1".into(),
        ])
        .unwrap_err();

        insta::assert_snapshot!(error, @"invalid Byobu tmux session: other");
    }
}

fn invoked_as(args: &[OsString], name: &str) -> bool {
    args.first()
        .and_then(|arg| Path::new(arg).file_name())
        .is_some_and(|arg| arg == name)
}

fn utf8_args(args: impl IntoIterator<Item = OsString>) -> Result<Vec<String>> {
    args.into_iter()
        .map(|arg| {
            arg.into_string()
                .map_err(|_| anyhow::anyhow!("arguments must be UTF-8"))
        })
        .collect::<Result<Vec<_>>>()
        .context("parse guest command arguments")
}
