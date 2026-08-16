# ADR 0040: Prohibit SSH agent forwarding into worlds

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

The forwarded agent is an unrestricted credential path. Code in a host world
can use it to access repositories and update branches outside the gateway's
`wt/*` policy. Maintaining it also makes host setup depend on a live workstation
connection and requires a stable guest-side socket that is retargeted after
each reconnect.

Omitting `ForwardAgent` from generated client configuration is not a complete
boundary because a developer can request forwarding with `ssh -A`.

## Decision

WT does not forward workstation SSH authentication agents into any world.

- Generated SSH configuration never enables `ForwardAgent`.
- Guest and devcontainer SSH servers set `AllowAgentForwarding no`.
- Host setup does not consume `SSH_AUTH_SOCK` and does not maintain a stable
  forwarded-agent socket for Byobu or cloud-init.
- The agent Git gateway is the only supported credential path for provider Git
  and API operations from a world. There is no compatibility option or opt-out.

SSH TCP forwarding with `-L`, `-R`, or `-D` is separate from authentication-agent
forwarding and remains available. WT may restrict it independently if its
threat model changes.

Existing worlds must be recreated to receive the SSH-server restriction. A
client sync removes automatic forwarding from existing aliases immediately,
but that alone does not prevent an explicit `ssh -A` connection to an old
world.

## Consequences

- All world kinds have one credential-isolation rule and one supported
  provider-access path.
- The gateway's branch and operation policies cannot be bypassed with the
  workstation agent.
- Host cloud-init continues independently after its initiating SSH connection
  closes.
- Host recipes cannot use workstation identities to access arbitrary private
  SSH services. A future credential mechanism for such services requires a
  separate design and does not reintroduce agent forwarding.
