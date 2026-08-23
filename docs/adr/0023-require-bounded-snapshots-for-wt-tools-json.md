# ADR 0023: Bound snapshots and cover wtg tools JSON

- Status: Accepted
- Date: 2026-08-21

## Decision

Committed `.snap` files must be at most 1,000 lines. Exact
repository-relative paths may be allowlisted as exceptions.

A static Rust check enforces this for every committed snapshot.

Each `wtg tools` command must have an Insta snapshot of its JSON response from
the real command path, using provider fixtures at the upstream boundary. Do not
construct `ProviderCommandOutput` in these tests.

## Consequences

- Snapshot growth is reviewed and bounded across the repository.
- New `wtg tools` commands need a JSON snapshot.
- JSON schema and unexpected upstream fields show in snapshot diffs.
