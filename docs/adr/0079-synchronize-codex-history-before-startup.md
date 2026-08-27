# ADR 0079: Synchronize Codex history before startup

- Status: Accepted
- Date: 2026-08-23

## Context

WT shares Codex rollout files across worlds while each world keeps a local Codex session
database. The background reconciliation design refreshed every running world's database when any
shared rollout changed. Active Codex sessions update rollouts frequently, causing repeated full
app-server scans while a user was working.

## Decision

The `codex` wrapper synchronizes the local session database immediately before starting the real
Codex CLI. It writes a progress message to standard output and names
`IGNORE_CODEX_WT_CHECKS=true` as the explicit bypass.

WT does not index or expose rollout metadata through the server. It only
coordinates shared filesystem history before local Codex startup and does not request
database reconciliation from running worlds. A process lock serializes concurrent startup
refreshes in one world.

## Consequences

Codex startup may wait for a local history refresh after shared history changes, but active Codex
sessions no longer trigger background scans or host-to-world reconciliation broadcasts.
