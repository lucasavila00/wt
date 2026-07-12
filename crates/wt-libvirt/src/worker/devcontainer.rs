//! Guest-side entrypoint for interactive shells in the primary devcontainer.

use super::guest_agent;
use crate::SessionFrontend;
use crate::WorkerError;
use std::io::Write;
use std::time::Instant;
use virt::domain::Domain;

pub(super) const APP_SHELL_PATH: &str = "/usr/local/bin/wt-app-shell";
pub(super) const APP_PANE_PATH: &str = "/usr/local/bin/wt-app-pane";
pub(super) const TMUX_CONFIG_PATH: &str = "/usr/local/share/wt-tmux.conf";
pub(super) const SESSION_FRONTEND_PATH: &str = "/usr/local/share/wt-session-frontend";

const TMUX_CONFIG: &[u8] = b"set-option -g default-command /usr/local/bin/wt-app-pane\n";
const INSTALL_MODES: &str = "chmod 0755 \"$1\" \"$2\" && chmod 0644 \"$3\" \"$4\"";

pub(super) fn install_app_tools(
    domain: &Domain,
    app_shell: &[u8],
    app_pane: &[u8],
    session: SessionFrontend,
    deadline: Instant,
    log: &mut dyn Write,
) -> Result<(), WorkerError> {
    guest_agent::write(domain, APP_SHELL_PATH, app_shell)?;
    guest_agent::write(domain, APP_PANE_PATH, app_pane)?;
    guest_agent::write(domain, TMUX_CONFIG_PATH, TMUX_CONFIG)?;
    guest_agent::write(
        domain,
        SESSION_FRONTEND_PATH,
        session_frontend_config(session),
    )?;
    guest_agent::run_phase(
        domain,
        "devcontainer shell and session configuration",
        "/bin/sh",
        &[
            "-c",
            INSTALL_MODES,
            "wt-app-tools",
            APP_SHELL_PATH,
            APP_PANE_PATH,
            TMUX_CONFIG_PATH,
            SESSION_FRONTEND_PATH,
        ],
        deadline,
        log,
    )?;
    Ok(())
}

fn session_frontend_config(session: SessionFrontend) -> &'static [u8] {
    match session {
        SessionFrontend::Tmux => b"tmux\n".as_slice(),
        SessionFrontend::Byobu => b"byobu\n".as_slice(),
    }
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

    #[test]
    fn session_frontend_is_strict_guest_configuration() {
        assert_eq!(session_frontend_config(SessionFrontend::Tmux), b"tmux\n");
        assert_eq!(session_frontend_config(SessionFrontend::Byobu), b"byobu\n");
    }
}
