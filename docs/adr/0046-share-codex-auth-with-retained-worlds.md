# ADR 0046: Make Codex a required retained-world integration

- Status: Accepted
- Date: 2026-08-21
- Supersedes: ADR 0044

## Context

Codex is part of the standard WT agent environment. Installing it in a client
host recipe, asking repositories to define container mounts, and configuring
server paths independently produces worlds with different capabilities.
Sharing only rollout files also leaves each environment's local Codex index
unaware of conversations created elsewhere.

Authentication must follow the same world lifecycle without copying a mutable
credential into world disks. Codex may replace `auth.json` atomically when the
server user logs in again, while worlds must remain read-only consumers.

## Decision

Codex is required for every retained host and devcontainer world. Both retained
images install the upstream Codex CLI. Provisioning installs `wt-codex-integration` and
activates its `codex` trampoline. The trampoline asks Codex to reconcile shared
rollouts into the environment's local index before starting the saved real CLI;
it never edits the index directly, and reconciliation failure warns without
blocking Codex startup.

The server `wt` user must already be logged in. Installation requires
`/home/wt/.codex/auth.json` to be a regular, non-symlink file owned by that
user. WT has exactly two fixed server-backed resources and no configuration for
additional paths:

- `/home/wt/.codex/sessions`, mounted read-write in retained worlds;
- `/home/wt/.codex/.wt-auth`, a WT-managed export containing only `auth.json`,
  mounted read-only in retained worlds.

The auth export is a hard link to the live server credential, not a copy. A
systemd path unit reruns the export helper after an atomic replacement. The
guest mounts the export directory at `/run/wt-codex-integration-auth` and links
`/home/wt/.codex/auth.json` to its file. Refreshing expired authentication is a
server-user operation; worlds cannot write it back.

Devcontainer setup injects Codex, `wt-codex-integration`, the read-write sessions path, and
the read-only auth export into the primary container. It links both resources
under the configured `remoteUser`'s `.codex` directory. Repositories do not own
or configure this integration.

Do not share the complete `.codex` directory. Databases, indexes, logs, locks,
and other runtime state remain local to each world and container. GitHub CI
runners do not receive the server's Codex data.

## Consequences

- Every retained environment has the same Codex CLI, login, session history,
  and reconciliation behavior without project configuration.
- Sessions outlive worlds and remain writable from every retained environment;
  users must not open one conversation concurrently in multiple worlds.
- The credential stays outside world disks, and mount consumers cannot modify
  it. Any trusted process in a retained world can still read and exfiltrate it,
  so the shared login has account-wide security impact.
- Codex image installation and the server login become hard prerequisites for
  building images and installing WT.
