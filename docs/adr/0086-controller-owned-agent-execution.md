# ADR 0086: Controller-owned agent execution

- Status: Accepted
- Date: 2026-09-05

## Context

Coupling agent sessions to WT made provider behavior, runtime upgrades, and
completion recovery part of world infrastructure. Controllers need to own
those policies independently of WT releases and golden-image rebuilds.

## Decision

WT owns worlds and a generic bounded command transport. It validates world ownership,
executes an absolute executable with argv/stdin as the guest user, and returns UTF-8
stdout, stderr, and exit status. It does not interpret or retry agent operations.

Controllers own agent runtime code, installation, versions, supervision, state,
provider adaptation, and result delivery. WT does not ship agapi or generated
consumer clients. WT images include the interactive Codex CLI, development
tools, and authentication/session mounts.

A controller prepares a running world through `exec_world`: install its runtime,
start it under a guest supervisor, and verify readiness. Installation and agent
work that exceed the transport deadline run under the supervisor and are polled
through subsequent bounded calls. No agent-specific WT endpoint is required.

## Consequences

Controllers can update their runtimes in existing worlds. They must supervise
long-running work and recover uncertain outcomes; WT's transport does not
provide durable agent execution or automatic mailbox completion delivery.
