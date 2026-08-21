# ADR 0049: Serialize Git proxy writes

- Status: Proposed
- Date: 2026-08-21

## Context

Git proxy administration rewrites the complete `authorized_keys` file. Concurrent
writes can lose changes, restore revoked keys, or expose partial files.

## Decision

Each installation owns one stable lock file beside its configuration. Every
state-changing admin and installer operation holds its exclusive `flock` from
the first state read through publication.
The lock file is created once and is never replaced, renamed, or unlinked.
All mutation paths use the same lock and publication helper.
Writers create unique temporary files in the destination directory, set final
ownership and mode, fsync them, rename them over the destination, then fsync
the directory. Readers do not take the lock.
Installers publish credentials before configuration. Published credentials remain
valid for the active configuration until configuration publication succeeds.
Failure before rename leaves the destination unchanged. Lock acquisition or
publication failure returns an error and does not continue the operation.

## Consequences

- Writers execute in one order without lost updates.
- Readers see a complete version of each file without blocking.
