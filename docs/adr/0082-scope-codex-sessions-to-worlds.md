# ADR 0082: Scope Codex sessions to worlds

- Status: Proposed
- Date: 2026-08-27

## Context

WT mounts one global Codex sessions tree into every world, while each world keeps a local state
database. Codex startup therefore indexes history created by every world.

On `gentle-falcon`, 459 rollouts totaling 1.05 GB exceeded WT's 30-second app-server timeout.
WT killed the backfill after 351 threads, leaving its state `running` with no worker alive. Later
Codex starts waited on that stale lease and failed. `IGNORE_CODEX_WT_CHECKS=true` skips WT's scan,
but not Codex's own backfill of every visible rollout.

## Decision

1. Remove prelaunch reconciliation and immediately execute the installed Codex CLI.
2. Stop mounting the global sessions tree into each world's Codex home.
3. Keep the read-only Codex authentication share unchanged.
4. Give each world a server-backed sessions directory keyed by its immutable world ID. Mount only
   that directory at `/home/wt/.codex/sessions` so sessions remain durable and isolated.
5. Delete the global sessions tree during the clean cutover. Do not migrate legacy sessions.
6. Later add explicit bounded import: copy or project one selected rollout from a source world's
   sessions directory into the target world, then run `codex resume <id>`.

Do not increase timeouts, share Codex's SQLite database, or run global background scans per world.
Each leaves work or coordination proportional to global history.

## Migration

Deploy after `make clear` or `make nuke`. Delete the global sessions tree and old local databases;
do not migrate or repair them. Rebuild the image, create new worlds with per-world mounts, and
update the existing Codex ADRs when this proposal is implemented.
