//! Persistent session entrypoint for the primary devcontainer.

use std::os::unix::process::CommandExt;
use std::process::Command;
use wt_command::cmd;

const TMUX_CONFIG: &str = "/usr/local/share/wt-tmux.conf";
const SESSION_FRONTEND: &str = "/usr/local/share/wt-session-frontend";

fn main() {
    let mut command = match command() {
        Ok(command) => command,
        Err(error) => {
            eprintln!("wt: start the persistent app session: {error}");
            std::process::exit(1);
        }
    };
    let error = command.exec();
    eprintln!("wt: start the persistent app session: {error}");
    std::process::exit(1);
}

fn command() -> Result<Command, String> {
    let frontend = std::fs::read_to_string(SESSION_FRONTEND)
        .map_err(|error| format!("read {SESSION_FRONTEND}: {error}"))?;
    command_for(frontend.trim())
}

fn command_for(frontend: &str) -> Result<Command, String> {
    let program = match frontend {
        "tmux" => "/usr/bin/tmux",
        "byobu" => "/usr/bin/byobu-tmux",
        value => return Err(format!("unsupported session frontend: {value}")),
    };
    Ok(cmd!(
        program,
        "-L",
        "wt-app",
        "-f",
        TMUX_CONFIG,
        "new-session",
        "-A",
        "-s",
        "wt-app",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attaches_or_creates_the_private_app_session() {
        let command = command_for("tmux").unwrap();
        assert_eq!(command.get_program(), "/usr/bin/tmux");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [
                "-L",
                "wt-app",
                "-f",
                "/usr/local/share/wt-tmux.conf",
                "new-session",
                "-A",
                "-s",
                "wt-app",
            ]
        );
    }

    #[test]
    fn starts_byobu_with_the_private_app_session() {
        let command = command_for("byobu").unwrap();
        assert_eq!(command.get_program(), "/usr/bin/byobu-tmux");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [
                "-L",
                "wt-app",
                "-f",
                "/usr/local/share/wt-tmux.conf",
                "new-session",
                "-A",
                "-s",
                "wt-app",
            ]
        );
    }

    #[test]
    fn rejects_unknown_session_frontends() {
        assert_eq!(
            command_for("screen").unwrap_err(),
            "unsupported session frontend: screen"
        );
    }
}
