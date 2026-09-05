# ADR 0086: Controller-owned agent execution

WT owns worlds and a generic bounded command transport. It validates world ownership,
executes an absolute executable with argv/stdin as the guest user, and returns UTF-8
stdout, stderr, and exit status. It does not interpret or retry agent operations.

Controllers own agent runtime code, installation, versions, supervision, state,
provider adaptation, and result delivery. Apr owns agapi. WT images include the
interactive Codex CLI, development tools, and authentication/session mounts.

A controller prepares a running world through `exec_world`: install its runtime,
start it under a guest supervisor, and verify readiness. Installation and agent
work that exceed the transport deadline run under the supervisor and are polled
through subsequent bounded calls. No agent-specific WT endpoint is required.
