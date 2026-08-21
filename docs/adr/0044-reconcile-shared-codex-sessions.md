# ADR 0044: Reconcile shared Codex sessions through Codex

- Status: Accepted
- Date: 2026-08-20
- Amends: [ADR 0042](0042-share-configured-folders-with-retained-worlds.md)

## Context

Sharing `~/.codex/sessions` preserves rollouts, but Codex also keeps a local
index. An already-running app does not show new shared rollouts until Codex
indexes them. Sharing Codex's databases would create unsafe concurrent writers.

## Decision

Keep sharing only `~/.codex/sessions`. Add a setup-owned shim that asks a
short-lived `codex app-server` to index missing rollouts with
`thread/list`, `thread/read`, then `thread/list` again. Probe the protocol and
use Codex's scan-and-repair listing as fallback. Never edit its index directly.
WT ships `wt-codex`. `wt-codex reconcile` runs the repair on demand.
`wt-codex install` replaces the `codex` command found in `PATH` with a shim
and keeps the real CLI at a private path. The shim reconciles, then executes the
real CLI unchanged. Failure warns but does not block Codex startup.
`wt-codex uninstall`, also available as `wt-codex remove`, restores the real
CLI.

WT does not control cloud-init recipes. A recipe may opt in by running
`wt-codex install`; WT's example cloud-init could demonstrate it.

## Consequences

Starting Codex runs one extra local command first. If that command fails, Codex
still starts. Tests verify that restored sessions appear and that the host and
guest keep separate indexes.
