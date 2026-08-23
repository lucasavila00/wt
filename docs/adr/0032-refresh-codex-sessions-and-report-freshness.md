# ADR 0032: Refresh Codex sessions and report freshness

- Status: Accepted; Date: 2026-08-21

## Decision

Codex inventory uses a dedicated `std::thread`, separate from the world refresh
worker described in ADR 0031. Both inventories retain their last complete
snapshot when any configured context cannot be queried. Codex also publishes
the query error so the shell can show why its displayed session state is stale.

The Codex worker fetches immediately, then starts each subsequent fetch five
seconds after the previous fetch finishes. Each context request has a one-minute
timeout. A stop channel and cancellation flag interrupt idle waits and running
requests during shell shutdown.

Snapshots cross to the UI thread through a one-slot MPSC channel. If the slot is
occupied, the worker drops the new snapshot and fetches current state again on
the next cycle. Fetches cannot overlap or accumulate work.

The Worlds and Codex panels store independent RFC 3339 UTC timestamps. The UI
updates a timestamp only when it applies a snapshot; it shows `Updating…` until
the first snapshot for that panel. A Codex query failure does not advance the
timestamp because no new snapshot is applied. The Codex title shows the failure
beside the last successful update time until a complete refresh succeeds.
