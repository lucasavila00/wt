# ADR 0055: Refresh Codex sessions and report freshness

- Status: Accepted; Date: 2026-08-21

## Decision

Codex inventory uses a dedicated `std::thread`, separate from the world refresh
worker described in ADR 0054. The two inventories have different acceptance
rules: Codex publishes per-context failures, while worlds require a complete
snapshot and reconcile SSH sessions.

The Codex worker fetches immediately, then starts each subsequent fetch five
seconds after the previous fetch finishes. Each context request has a one-minute
timeout. A stop channel and cancellation flag interrupt idle waits and running
requests during shell shutdown.

Snapshots cross to the UI thread through a one-slot MPSC channel. If the slot is
occupied, the worker drops the new snapshot and fetches current state again on
the next cycle. Fetches cannot overlap or accumulate work.

The Worlds and Codex panels store independent RFC 3339 UTC timestamps. The UI
updates a timestamp only when it applies a snapshot; it shows `Updating…` until
the first snapshot for that panel. A Codex snapshot containing context failures
still advances its timestamp because the failure rows are the applied result.
