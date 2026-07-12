//! Persistent tmux entrypoint for the primary devcontainer.

use std::os::unix::process::CommandExt;
use std::process::Command;

const TMUX_CONFIG: &str = "/usr/local/share/wt-tmux.conf";

fn main() {
    let error = command().exec();
    eprintln!("wt: start the persistent app session: {error}");
    std::process::exit(1);
}

fn command() -> Command {
    let mut command = Command::new("/usr/bin/tmux");
    command.args([
        "-L",
        "wt-app",
        "-f",
        TMUX_CONFIG,
        "new-session",
        "-A",
        "-s",
        "wt-app",
    ]);
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attaches_or_creates_the_private_app_session() {
        let command = command();
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
}
