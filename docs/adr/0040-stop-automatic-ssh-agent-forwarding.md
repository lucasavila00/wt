# ADR 0040: Stop automatic SSH agent forwarding

- Status: Accepted
- Date: 2026-08-16
- Amends: [ADR 0017](0017-integrate-agent-git-gateway.md),
  [ADR 0026](0026-make-world-kinds-first-class.md), and
  [ADR 0037](0037-give-host-worlds-broad-agent-git-access.md)

## Context

Devcontainer worlds use the agent Git gateway and do not receive the
workstation's SSH authentication agent by default. Host worlds use the same
gateway but also forward the workstation agent into direct SSH and persistent
Byobu sessions.

The automatically forwarded agent is an unrestricted credential path. Code in
a host world can use it to access repositories and update branches outside the
gateway's `wt/*` policy. Maintaining it also makes host setup depend on a live
workstation connection and requires a stable guest-side socket that is
retargeted after each reconnect.

## Decision

WT does not automatically forward workstation SSH authentication agents into
any world.

- Generated SSH configuration never enables `ForwardAgent`.
- Host setup does not consume `SSH_AUTH_SOCK` and does not maintain a stable
  forwarded-agent socket for Byobu or cloud-init.
- The agent Git gateway is the only WT-managed credential path for provider Git
  and API operations from a world.

WT does not disable OpenSSH's native authentication-agent forwarding. A
developer may explicitly use `ssh -A NAME-vs` for an individual direct
connection. That agent is an unrestricted credential path which bypasses
gateway policy and remains the developer's responsibility.

Using `ssh -A NAME` with the persistent Byobu session is unsupported. Existing
panes may retain a stale socket after the SSH connection closes or a later
connection attaches. WT does not preserve or refresh that socket, expose it to
host setup, or carry it across another SSH hop.

SSH TCP forwarding with `-L`, `-R`, or `-D` is separate from authentication-agent
forwarding and remains available.

Existing aliases stop forwarding automatically after a client sync. Existing
host worlds must be recreated to remove their guest-side stable-socket
machinery.

## Consequences

- All world kinds use the gateway by default and do not receive workstation
  credentials through normal WT commands.
- Host cloud-init continues independently after its initiating SSH connection
  closes.
- Host recipes cannot rely on a workstation agent. Public resources and
  provider Git operations through the gateway remain available.
- Developers retain an explicit, unsupported OpenSSH escape hatch for
  exceptional interactive use without making it part of the WT lifecycle
  contract.
