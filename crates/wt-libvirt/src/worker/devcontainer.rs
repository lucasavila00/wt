//! Guest-side entrypoint for interactive shells in the primary devcontainer.

use super::guest_agent;
use crate::WorkerError;
use std::io::Write;
use std::time::Instant;
use virt::domain::Domain;

pub(super) const APP_SHELL_PATH: &str = "/usr/local/bin/wt-app-shell";
pub(super) const APP_PANE_PATH: &str = "/usr/local/bin/wt-app-pane";
pub(super) const TMUX_CONFIG_PATH: &str = "/usr/local/share/wt-tmux.conf";

const TMUX_CONFIG: &[u8] = b"set-option -g default-command /usr/local/bin/wt-app-pane\n";
const INSTALL_MODES: &str = "chmod 0755 \"$1\" \"$2\" && chmod 0644 \"$3\"";

pub(super) fn install_app_tools(
    domain: &Domain,
    app_shell: &[u8],
    app_pane: &[u8],
    deadline: Instant,
    log: &mut dyn Write,
) -> Result<(), WorkerError> {
    guest_agent::write(domain, APP_SHELL_PATH, app_shell)?;
    guest_agent::write(domain, APP_PANE_PATH, app_pane)?;
    guest_agent::write(domain, TMUX_CONFIG_PATH, TMUX_CONFIG)?;
    guest_agent::run_phase(
        domain,
        "devcontainer shell and tmux configuration",
        "/bin/sh",
        &[
            "-c",
            INSTALL_MODES,
            "wt-app-tools",
            APP_SHELL_PATH,
            APP_PANE_PATH,
            TMUX_CONFIG_PATH,
        ],
        deadline,
        log,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tmux_pane_enters_the_devcontainer() {
        assert_eq!(
            TMUX_CONFIG,
            b"set-option -g default-command /usr/local/bin/wt-app-pane\n"
        );
    }
}
