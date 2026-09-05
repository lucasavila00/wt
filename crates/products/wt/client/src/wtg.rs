use anyhow::{bail, Context, Result};
use std::ffi::OsString;
use std::path::Path;

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
        Some("tools") => wt_agent_tool_gateway::tools_command::run(utf8_args(args.drain(2..))?),
        Some(other) => bail!("unknown guest command {other:?}; run `wtg help` for usage"),
        None => bail!("missing guest command; run `wtg help` for usage"),
    }
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
