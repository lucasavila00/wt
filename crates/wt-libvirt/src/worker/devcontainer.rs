//! Guest-side entrypoint for interactive shells in the primary devcontainer.

use super::guest_agent;
use crate::WorkerError;
use std::time::Instant;
use virt::domain::Domain;

pub(super) const APP_SHELL_PATH: &str = "/usr/local/bin/wt-app-shell";

pub(super) fn install_app_shell(
    domain: &Domain,
    app_shell: &[u8],
    deadline: Instant,
) -> Result<(), WorkerError> {
    guest_agent::write(domain, APP_SHELL_PATH, app_shell)?;
    guest_agent::run_phase(
        domain,
        "devcontainer shell",
        "/bin/chmod",
        &["0755", APP_SHELL_PATH],
        deadline,
    )?;
    Ok(())
}
