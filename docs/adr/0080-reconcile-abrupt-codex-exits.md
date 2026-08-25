# ADR 0080: Reconcile abrupt Codex exits from pane liveness

- Status: Accepted
- Date: 2026-08-23

## Context

Codex lifecycle hooks are best-effort. A normal `SessionEnd` hook marks a
session inactive, but abruptly interrupting Codex can terminate its process
after a `Stop` hook reports `needs_attention` and before `SessionEnd` runs.
The shell then continues to show a stale `NEEDS ATTENTION` card even though
Codex has returned to the pane's shell.

The guest relay already persists each accepted lifecycle observation as a
registration and polls its tmux pane every two seconds to track Git context.
The registration includes the pane generation and most recently accepted event
sequence, while the pane marker identifies the currently registered session.

## Decision

During its existing tracker pass, the guest relay reads the marked pane's
foreground command. It keeps the session active while that command is `codex`.
When the same pane marker remains but the foreground command has changed, the
relay infers that Codex exited without reporting `SessionEnd`.

The relay sends one synthetic `SessionEnd` with the next event sequence, then
clears the matching pane marker and removes the registration. The registry's
normal event ordering and state transition mark the session inactive. Relay
transport or tmux failures do not infer an exit; the registration is retried or
retired using the pre-existing missing-marker behavior.

Persisted registrations from before sequence tracking lack a safe sequence to
use. The relay removes those legacy registrations when their marked pane has
closed rather than fabricating an ordered lifecycle event.

## Consequences

- A Ctrl-C or other abrupt Codex exit clears a stale `NEEDS ATTENTION` state
  within the existing two-second relay interval.
- A live Codex prompt remains `NEEDS ATTENTION`; pane liveness supplements,
  rather than replaces, the `Stop` hook.
- The relay makes no additional network connection, worker, or polling loop.
- A resumed session that uses a new pane generation remains protected from
  delayed lifecycle events by the existing ordering fence.
