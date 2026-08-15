# ADR 0033: Forward SSH agents to host worlds

- Status: Accepted
- Date: 2026-08-15
- Amends: [ADR 0026](0026-make-world-kinds-first-class.md)

## Context

Host worlds use normal Git and do not have the agent Git gateway. Their SSH
aliases should forward the local SSH agent.

Byobu panes survive SSH reconnects. The forwarded agent socket does not.
OpenSSH creates a new socket for each connection and deletes it on disconnect.
A surviving shell still points to the deleted socket.

## Decision

Generated host aliases enable SSH agent forwarding. This applies to the Byobu
alias and the direct `-vs` alias. Devcontainer aliases remain unchanged.

The Byobu shell uses one stable socket path. Each Byobu login points that path
to the new forwarded socket. Existing panes then use the new agent. If two
connections overlap, the latest login wins.

There is no setting for this. Anyone controlling a host world can use the agent
while the SSH connection is open. WT does not copy private keys.
