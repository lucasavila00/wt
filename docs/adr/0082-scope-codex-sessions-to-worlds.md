# ADR 0082: Scope Codex sessions to worlds

- Status: Accepted
- Date: 2026-08-27

## Context

Mounting global Codex history into every world made local startup indexing
proportional to all worlds' rollouts. Large histories exceeded startup timeouts
and left stale backfill state. Increasing timeouts or sharing the SQLite
database would retain the global work and coordination problem.

## Decision

Give each world a server-backed sessions directory keyed by its immutable
world ID. Mount only that directory read-write at `/home/wt/.codex/sessions`.
Keep databases, indexes, locks, logs, and configuration local to the world.
Authentication remains a separate read-only share (ADR 0020).

Start the installed interactive Codex CLI directly. WT performs no prelaunch
history reconciliation or global background scans. Transferring a retained
conversation is an explicit copy of that conversation between worlds, not a
shared history mount or automatic synchronization.

## Consequences

Session rollouts survive world stops, while each world's indexing work depends
only on its own visible history. Session storage does not supply live pane
state or controller-owned agent execution state.
