# ADR 0076: Expire transient Codex session phases

- Status: Accepted
- Date: 2026-08-23

## Problem

Codex lifecycle hooks are short-timeout, best-effort notifications. WT must not
block Codex when a report cannot reach the host. Durable lifecycle observations
therefore cannot assume that every starting event has a matching completion
event.

WT stored compaction as a boolean set by `PreCompact` or a compact
`SessionStart` and cleared by a later lifecycle hook. If the clearing hook was
lost, timed out, or rejected, the shell displayed `COMPACTING` indefinitely.
Ordering events prevents old reports from overwriting new ones, but cannot
recover an event that was never accepted.

## Decision

Treat transient, hook-derived phases as bounded observations rather than
durable truth. The registry retains the reported compaction value and receipt
time, but exposes `is_compacting` only for two minutes after the report that set
it. The lease expires at exactly two minutes. Future-dated reports are not
treated as current.

Every accepted lifecycle event continues to clear compaction unless that event
explicitly starts or continues the phase. Expiry is derived when reports are
read; it does not require cleanup writes or a database migration, and existing
stuck observations converge automatically.

Deployment uses the normal `make clear` runtime reset, so hook-order files from
the earlier generation format are not migrated.

Pane generations remain the causal fence for lifecycle events. A session that
returns to a pane receives a new generation, so current completion events are
not rejected merely because the same session UUID occupied that pane before.

## Consequences

The shell can stop showing `COMPACTING` during a genuinely slow compaction. That
is preferable to presenting an unauthoritative UI hint forever. Primary
lifecycle state remains separate and unchanged.

This decision fixes the starting-edge-without-completion failure mode for the
compaction phase and establishes the policy for similar transient phases.
Long-lived states such as `working`, `needs_attention`, and `inactive` still
require authoritative process and pane liveness reconciliation; they must not
receive arbitrary expiry policies.
