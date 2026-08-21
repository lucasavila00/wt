# ADR 0054: Refresh the shell world list

- Status: Accepted; Date: 2026-08-21

## Context

`wt shell` originally read its worlds only at startup. A world created or
removed in another terminal therefore left the open shell out of date.

## Decision

The shell lists worlds in a background worker every five seconds. Successful
complete results reconcile the visible list and SSH sessions without blocking
terminal input.

World UUIDs identify sessions. Names remain display labels and SSH destinations,
because a removed world can be recreated with the same name and a new identity.

New worlds get a new SSH session. Removed worlds lose their session. The active
world stays selected while its UUID remains present; otherwise the first world
becomes active. If no worlds remain, the shell shows its empty Control UI.

If any context cannot be listed or managed SSH configuration cannot be synced,
the shell keeps its last complete list and tries again later.

## Consequences

Changes made outside the shell normally appear within five seconds. Each open
shell also performs a small, regular amount of control-plane and SSH setup work.
