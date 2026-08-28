# ADR 0083: Show Byobu window names for Codex panes

- Status: Accepted
- Date: 2026-08-28

## Context

World cards identify an observed Codex pane with its tmux target, such as
`Codex wt-host:%1 · CHANGING`. The fixed `wt-host` session name and opaque pane
ID are useful for exact targeting but do not help a user recognize the Byobu
window. Pane IDs also repeat across worlds because each world has its own tmux
server, which makes the labels appear ambiguous.

Byobu presents the same workspace through window indexes and automatically
maintained window names, such as `0:codex` and `1:make`. WT should use that
familiar vocabulary for presentation while retaining the pane target for
operations.

## Decision

Collect the tmux `window_index` and `window_name` with every observed Codex
pane. Carry both required fields through the guest relay, control protocol,
registry, server-owned observation, and shell model. The window index is a
non-negative integer. The window name is bounded, normalized display text.

Render the window name in user-facing Codex labels:

```text
Codex · window “codex” · CHANGING
Codex · window “make” · STATIC
```

Use the window index for ordering and disambiguation where required. Do not
show the tmux session name or pane ID in the normal card label. Continue to use
`(world, tmux session, pane ID)` as the exact observation and focus identity;
window indexes and names are mutable presentation metadata, not identity.

This is one incompatible cutover after `make clear`. Bump the protocol, require
both new fields, and recreate registry state. Do not accept observations
without window metadata, add compatibility defaults, or migrate old pane
observation rows.

## Consequences

- Codex labels match the window names users see in Byobu instead of exposing
  internal tmux targets.
- Automatic or manual tmux window renames appear on the next observation.
- Multiple worlds may use the same window index or name without confusing
  operational identity.
- Every observation carries two additional fields across the guest, server,
  persistence, and client boundaries.
