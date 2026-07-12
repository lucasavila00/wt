//! Guest-side entrypoint for interactive shells in the primary devcontainer.

use super::guest_agent;
use crate::SessionFrontend;
use crate::WorkerError;
use std::io::Write;
use std::time::Instant;
use virt::domain::Domain;

pub(super) const APP_SHELL_PATH: &str = "/usr/local/bin/wt-app-shell";
pub(super) const APP_PANE_PATH: &str = "/usr/local/bin/wt-app-pane";
pub(super) const APP_INFO_PATH: &str = "/usr/local/bin/wt-app-info";
pub(super) const APP_PROXY_PATH: &str = "/usr/local/bin/wt-app-proxy";
pub(super) const TMUX_CONFIG_PATH: &str = "/usr/local/share/wt-tmux.conf";
pub(super) const SESSION_FRONTEND_PATH: &str = "/usr/local/share/wt-session-frontend";
pub(super) const APP_SSH_PUBLIC_DIR: &str = "/var/lib/wt-app-ssh/public";
pub(super) const APP_SSH_MOUNT: &str = "/run/wt-app-ssh";
pub(super) const APP_SSH_PORT: u16 = 2222;
pub(super) const SSHD_FEATURE: &str = "ghcr.io/devcontainers/features/sshd:1.1.0";

const TMUX_CONFIG: &[u8] = b"set-option -g default-command /usr/local/bin/wt-app-pane\n";
const SSHD_CONFIG: &[u8] = b"Port 2222\nHostKey /run/wt-app-ssh/ssh_host_ed25519_key\nPidFile /run/sshd-wt.pid\nAuthorizedKeysFile /run/wt-app-ssh/authorized_keys/%u\nPasswordAuthentication no\nKbdInteractiveAuthentication no\nChallengeResponseAuthentication no\nUsePAM yes\nPermitRootLogin prohibit-password\nAllowTcpForwarding yes\nGatewayPorts no\nX11Forwarding no\nPrintMotd no\nStrictModes yes\nAcceptEnv LANG LC_*\nSubsystem sftp internal-sftp\n";
const INSTALL_MODES: &str = "chmod 0755 \"$1\" \"$2\" \"$3\" \"$4\" && chmod 0644 \"$5\" \"$6\"";

pub(super) struct AppTools<'a> {
    pub app_shell: &'a [u8],
    pub app_pane: &'a [u8],
    pub app_info: &'a [u8],
    pub app_proxy: &'a [u8],
}

pub(super) fn install_app_tools(
    domain: &Domain,
    tools: &AppTools<'_>,
    session: SessionFrontend,
    deadline: Instant,
    log: &mut dyn Write,
) -> Result<(), WorkerError> {
    guest_agent::write(domain, APP_SHELL_PATH, tools.app_shell)?;
    guest_agent::write(domain, APP_PANE_PATH, tools.app_pane)?;
    guest_agent::write(domain, APP_INFO_PATH, tools.app_info)?;
    guest_agent::write(domain, APP_PROXY_PATH, tools.app_proxy)?;
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
            APP_INFO_PATH,
            APP_PROXY_PATH,
            TMUX_CONFIG_PATH,
            SESSION_FRONTEND_PATH,
        ],
        deadline,
        log,
    )?;
    Ok(())
}

pub(super) fn prepare_app_ssh(
    domain: &Domain,
    deadline: Instant,
    log: &mut dyn Write,
) -> Result<(), WorkerError> {
    guest_agent::run_phase(
        domain,
        "app SSH key generation",
        "/bin/sh",
        &[
            "-c",
            "set -eu\ninstall -d -m 0700 -o wt -g wt /var/lib/wt-app-ssh\ninstall -d -m 0755 /var/lib/wt-app-ssh/public /var/lib/wt-app-ssh/public/authorized_keys\nssh-keygen -q -t ed25519 -N '' -f /var/lib/wt-app-ssh/public/ssh_host_ed25519_key\nssh-keygen -q -t ed25519 -N '' -f /var/lib/wt-app-ssh/session_identity\nchown wt:wt /var/lib/wt-app-ssh/session_identity /var/lib/wt-app-ssh/session_identity.pub\nchmod 0600 /var/lib/wt-app-ssh/public/ssh_host_ed25519_key /var/lib/wt-app-ssh/session_identity\nchmod 0644 /var/lib/wt-app-ssh/public/ssh_host_ed25519_key.pub /var/lib/wt-app-ssh/session_identity.pub",
        ],
        deadline,
        log,
    )?;
    guest_agent::write(
        domain,
        &format!("{APP_SSH_PUBLIC_DIR}/sshd_config"),
        SSHD_CONFIG,
    )?;
    guest_agent::run_phase(
        domain,
        "app SSH configuration permissions",
        "/bin/chmod",
        &["0644", &format!("{APP_SSH_PUBLIC_DIR}/sshd_config")],
        deadline,
        log,
    )
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
