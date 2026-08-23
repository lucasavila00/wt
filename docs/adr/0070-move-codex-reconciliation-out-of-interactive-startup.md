# ADR 0070: Reconcile Codex sessions in the background

- Status: Accepted
- Date: 2026-08-23
- Amends: [ADR 0037](0037-use-a-fixed-codex-prelaunch-wrapper.md)

## Context

WT shares Codex session files across worlds. Each world has its own Codex database, which must be
updated when shared history changes.

Today the `codex` wrapper does that work before starting Codex. It starts an app-server, waits 20
seconds, and stops it if reconciliation is still running.

Codex `0.149.0` can spend longer than 20 seconds backfilling its database. Stopping the app-server
leaves the backfill marked `running` until its 15-minute lease expires. Other Codex processes wait
30 seconds and then fail. That is why the problem later clears by itself.

Changing either timeout would only move the failure. Reconciliation is background maintenance and
should not be owned by an interactive command.

## Decision

`wt-server` becomes the single coordinator for Codex session reconciliation. The database work
still runs inside each world because Codex databases are local to world disks.

The server already maintains a catalog of shared sessions. When that catalog changes, or when a
world starts, the server asks each running world to reconcile its local database.

Each world runs at most one reconciliation job at a time. The job is independent of the command
that requested it and is allowed to finish. Its result identifies the shared catalog state and
Codex version that were applied.

The `codex` wrapper becomes a readiness check only:

- ready: start Codex immediately;
- reconciling: return immediately and report progress;
- failed or not prepared: return immediately with the current problem;
- explicit bypass: start Codex directly, without WT reconciliation guarantees.

The wrapper no longer scans sessions, starts an app-server, waits for reconciliation, or stops
reconciliation.

There is no permanent reconciliation daemon in each world. There is one coordinator in
`wt-server` and temporary database work in affected worlds.

Session files remain shared and canonical. Databases, migrations, indexes, and coordination state
remain local to each world. A host Codex app-server is not used because it would update the host
database, not a world's database.

## Why this shape

A lock prevents duplicate work, but it does not move reconciliation out of startup or keep a
timed-out backfill alive. It may support reconciliation, but it cannot own it.

A timestamp says when something ran, not what completed. It cannot prove that a particular session
catalog and Codex version were applied successfully.

The server already sees the shared history and knows which worlds are running. Coordinating there
avoids repeating that responsibility in every world while keeping Codex-owned state inside the VM.

## Consequences

Normal Codex startup no longer waits for WT reconciliation, and its latency does not grow with
shared session history.

A new or upgraded world may need background preparation before Codex is ready. Attempts made during
that window return immediately with useful status.

Failure in one world does not lock or corrupt another world's database. ADR 0060's guarantee changes
from reconciling before every launch to keeping running worlds reconciled in the background and
making readiness explicit.
