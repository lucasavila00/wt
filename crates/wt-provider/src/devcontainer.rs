//! Guest-side devcontainer and app SSH constants.

pub(super) const APP_INFO_PATH: &str = "/usr/local/bin/wt-app-info";
pub(super) const APP_SSH_PUBLIC_DIR: &str = "/var/lib/wt-app-ssh/public";
pub(super) const APP_SSH_MOUNT: &str = "/run/wt-app-ssh";
pub(super) const APP_SSH_PORT: u16 = 2222;
pub(super) const SSHD_FEATURE: &str = "ghcr.io/devcontainers/features/sshd:1.1.0";
