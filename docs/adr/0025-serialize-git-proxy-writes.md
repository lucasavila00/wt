# ADR 0025: Serialize Git proxy processes

- Status: Accepted
- Date: 2026-08-21

## Context

Git proxy commands share configuration, credentials, and `authorized_keys`.
Concurrent processes can read partial updates or overwrite each other's work.

## Decision

The installation lock is `/etc/wt-git-proxy.lock`.

`wt-git-proxy` and `wt-git-proxy-installer` take an exclusive `flock` when the
binary starts. They hold the lock until the process exits. A process waits when
another process owns the lock.

The lock file stays at a fixed path and is never replaced or removed.

## Consequences

- Only one Git proxy or installer process runs at a time.
- Reads and writes cannot overlap another process.
- Existing file operations need no additional locking.
