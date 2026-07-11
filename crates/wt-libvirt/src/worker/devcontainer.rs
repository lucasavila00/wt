//! Guest-side entrypoint for interactive shells in the primary devcontainer.

use super::guest_agent;
use crate::WorkerError;
use std::time::Instant;
use virt::domain::Domain;

pub(super) const APP_SHELL_PATH: &str = "/usr/local/bin/wt-app-shell";

const APP_SHELL: &[u8] = br#"#!/bin/sh
set -eu

containers=$(docker ps \
  --filter label=devcontainer.local_folder=/workspace \
  --format '{{.ID}}')
if [ -z "$containers" ]; then
  echo 'wt: the devcontainer app is not running' >&2
  exit 1
fi
if [ "$(printf '%s\n' "$containers" | wc -l)" -ne 1 ]; then
  echo 'wt: multiple devcontainer app containers match /workspace' >&2
  exit 1
fi
container=$containers

workspace=$(docker inspect --format \
  '{{range .Mounts}}{{if eq .Source "/workspace"}}{{.Destination}}{{end}}{{end}}' \
  "$container")
if [ -z "$workspace" ]; then
  echo 'wt: the devcontainer app does not mount /workspace' >&2
  exit 1
fi

metadata=$(docker inspect --format \
  '{{index .Config.Labels "devcontainer.metadata"}}' "$container")
user=
if [ "$metadata" != '<no value>' ]; then
  user=$(/usr/bin/node -e '
    const entries = JSON.parse(process.argv[1]);
    let containerUser = "";
    let remoteUser = "";
    for (const entry of entries) {
      if (typeof entry.containerUser === "string") containerUser = entry.containerUser;
      if (typeof entry.remoteUser === "string") remoteUser = entry.remoteUser;
    }
    process.stdout.write(remoteUser || containerUser);
  ' "$metadata")
fi

set -- -it --workdir "$workspace"
if [ -n "$user" ]; then
  set -- "$@" --user "$user"
fi
exec docker exec "$@" "$container" /bin/sh
"#;

pub(super) fn install_app_shell(domain: &Domain, deadline: Instant) -> Result<(), WorkerError> {
    guest_agent::write(domain, APP_SHELL_PATH, APP_SHELL)?;
    guest_agent::run_phase(
        domain,
        "devcontainer shell",
        "/bin/chmod",
        &["0755", APP_SHELL_PATH],
        deadline,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_shell_uses_devcontainer_identity_and_workspace() {
        let script = String::from_utf8_lossy(APP_SHELL);
        assert!(script.contains("label=devcontainer.local_folder=/workspace"));
        assert!(script.contains("remoteUser"));
        assert!(script.contains("--workdir"));
        assert!(script.contains("docker exec"));
    }
}
