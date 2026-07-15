# ADR 0003: Forward the workstation SSH agent to devcontainers

- Status: Accepted
- Date: 2026-07-15

## Context

WT forwards the workstation SSH agent to the guest only during setup. The
installer uses it to clone the repository, then removes `SSH_AUTH_SOCK` from
the process and Byobu environments. Running-world SSH config also removes
`ForwardAgent yes`.

The devcontainer never receives the workstation agent. Git and other SSH
commands inside it cannot use the user's loaded identities.

This creates two credential paths: the forwarded agent for the initial clone,
and user-managed credentials for later work in the devcontainer.

## Decision

Forward the workstation SSH agent for the lifetime of each user connection.

- Keep `ForwardAgent yes` on `NAME` aliases after setup.
- Set `ForwardAgent yes` on `NAME-vs` aliases.
- Keep `SSH_AUTH_SOCK` in the Byobu environment after setup.
- Refresh the Byobu `SSH_AUTH_SOCK` on every `ssh NAME` connection.
- Make `wt-app-pane` forward the agent on its guest-to-devcontainer SSH
  connection.

The guest-held session key remains. It authenticates `wt-app-pane` to the
devcontainer. The workstation agent supplies user identities to processes
inside the devcontainer.

When the workstation disconnects, its forwarded socket becomes unusable.
Existing shells remain alive. SSH operations fail until the user reconnects.
The reconnect replaces the stale socket in the Byobu environment. New panes
inherit the new socket.

## Verification

- Setup clones through the forwarded agent.
- `ssh NAME` exposes the workstation agent inside the devcontainer.
- `ssh NAME-vs` exposes the workstation agent inside the devcontainer.
- Disconnecting does not stop existing Byobu shells.
- Reconnecting replaces the stale agent socket.
- A pane created after reconnect uses the new agent socket.

## Consequences

- Initial clone and later Git operations use the same credential source.
- Users control prompting and confirmation through their workstation agent.
- No private key or passphrase is copied into WT state.
- Trusted guest and devcontainer processes can use the forwarded agent while
  the workstation connection is active.
- A passphrase-protected key may remain unlocked in an agent. Users who require
  approval for each operation must configure agent confirmation or a
  hardware-backed key policy.

## Alternatives

### Forward the agent only during setup

Rejected. Users need another credential setup for normal devcontainer work.

### Copy SSH keys into the devcontainer

Rejected. WT would persist private credentials in the world.

### Forward only through `NAME-vs`

Rejected. Normal Byobu panes opened through `ssh NAME` would still lack the
agent.
