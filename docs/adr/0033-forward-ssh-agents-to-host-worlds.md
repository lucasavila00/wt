# ADR 0033: Forward SSH agents to host worlds

- Status: Accepted
- Date: 2026-08-15
- Amends: [ADR 0026](0026-make-world-kinds-first-class.md)

## Context

Host worlds have Git but no agent Git gateway. Requiring `ssh -A` on every
connection makes normal private Git access unnecessarily awkward.

Byobu outlives an SSH connection. It cannot keep using OpenSSH's temporary agent
socket after that connection closes.

## Decision

Generated host aliases enable native SSH agent forwarding. This applies to the
regular Byobu alias and the direct `-vs` alias. Devcontainer aliases remain
unchanged and use the agent Git gateway.

The host shell exposes a stable per-user socket path to Byobu and refreshes its
target on every Byobu connection. Existing panes use the next forwarded agent
without being recreated. Concurrent sessions are last-connection-wins.

There is no setting or opt-out. A host world is a trusted personal machine.
Anyone controlling it can use the forwarded agent while a connection is live.
WT never copies the private keys.
