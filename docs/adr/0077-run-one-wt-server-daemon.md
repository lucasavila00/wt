# ADR 0077: Run one WT server daemon

- Status: Accepted
- Date: 2026-08-23

## Context

A WT server ran `wt-server` and `wt-agent-tool-gateway` as separate
long-lived services. It also installed systemd path and oneshot units to publish
Codex authentication and SSH authorized-key changes. Codex reconciliation had
already moved into the server, but Git and provider access, file watching, and
the control plane still had separate operational lifecycles.

These processes were installed, upgraded, restarted, diagnosed, and ordered as
one WT system. Their split added service dependencies and several places to look
for health and logs without providing an availability boundary: a usable WT
server required both persistent services.

## Decision

Run one persistent server service, `wts.service`. `wts` owns:

- the client control socket and guest lifecycle;
- the agent-tool vsock gateway for Git and provider operations;
- Codex session cataloging; and
- watching and atomically publishing shared Codex authentication and SSH
  authorized keys.

Remove `wt-agent-tool-gateway.service` and the path-triggered Codex-auth and
SSH-key service pairs. The server starts each responsibility during startup,
reports its readiness as part of server readiness, and stops all of them during
one orderly shutdown. A failure that prevents a required responsibility from
running fails the server instead of leaving a partially available installation.

Keep internal components and task boundaries. Consolidating service ownership
does not require one event loop, shared mutable state, or unstructured error
handling. Blocking Git, provider, file, and guest operations remain isolated
from one another by bounded worker tasks.

The combined service receives the credentials required by all of these
responsibilities. Provider credentials remain encrypted systemd credentials
and are opened only by the component that needs them. The server user and
existing Unix and vsock protocol authorization remain unchanged.

The standalone `wt-git-proxy` is not part of this service. It is a separate
product invoked by OpenSSH for an individual Git request and has no WT guest,
registry, or Codex responsibilities.

## Consequences

- Installation, upgrade, restart, health inspection, and logs have one server
  service boundary.
- Startup validates the required server runtime before reporting ready.
- A server restart also interrupts Git and provider operations, and a fatal
  gateway or watcher failure restarts the control plane. The implementation
  must contain request failures and reserve process failure for a responsibility
  that cannot be restored safely.
- The server process has access to more capabilities and credentials than it
  did before. This is accepted because the former persistent WT services ran as
  the same `wt` identity and together formed one required trusted system.
