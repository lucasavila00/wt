# ADR 0074: Track Codex Git state independently

- Status: Accepted
- Date: 2026-08-23

## Decision

Codex lifecycle hooks report only session identity, pane ordering, and activity.
The guest relay registers accepted active sessions, polls their pane-validated
working directories for Git context, and sends separate authenticated updates.

The host accepts a Git update only for the exact active world, session, cwd,
pane, and generation. It updates repository metadata and Git health without
changing lifecycle state, order, or lifecycle receipt time.

## Consequences

Branch switches appear without another Codex hook. Cards can warn when Git
state is unavailable or stale while preserving activity age. The incompatible
schema cutover is defined by ADR 0075 and requires `make clear`.
