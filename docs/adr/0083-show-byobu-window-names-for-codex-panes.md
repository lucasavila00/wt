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

Collect the tmux `window_index` and `window_name` with each rendered Codex pane
frame. Bundle them with that frame as one `PaneRender` value. The guest gateway
keeps each world's complete latest pane observations in one bounded in-memory
snapshot: exact pane target, fingerprint, timestamps, current directory,
optional Git branch, and render data. The server enriches those observations
with owner-scoped world names and creation times when serving the client.

Do not persist any pane observation. Live terminal state does not need to
survive a server crash, and a persisted observation was already incomplete
without its memory-only frame. A server restart, world stop, or grant revocation
clears the affected pane observations; cards return after the relay's next
report from a running world. The window index is a non-negative integer. The
window name is bounded, normalized display text.

Stopping or losing a world marks its in-memory pane slot inactive while clearing
the snapshot and advances its run generation, so an in-flight relay report
cannot restore stale data across a stop and restart. A successful start or
running-world reconciliation activates the next generation. Lifecycle
reconciliation uses the same per-world operation lock as start, stop, and
delete.

Render the window name in user-facing Codex labels:

```text
Codex · window “codex” · CHANGING
Codex · window “make” · STATIC
```

Use an available window index for ordering and disambiguation where required.
Do not show the tmux session name or pane ID in the normal card label. Continue
to use `(world, tmux session, pane ID)` as the exact observation and focus
identity. `PaneRender` is explicitly presentation data inside the transient
observation, not identity or lifecycle data.

This is one incompatible cutover after `make clear`. Bump both protocols,
require `PaneRender` on client-facing observations, and remove the
`pane_observations` table rather than migrating its rows.

## Consequences

- Codex labels match the window names users see in Byobu instead of exposing
  internal tmux targets.
- Automatic or manual tmux window renames appear on the next observation.
- Multiple worlds may use the same window index or name without confusing
  operational identity.
- Every pane observation has one in-memory lifetime and disappears completely
  on server restart.
- Window metadata has the same lifetime as its rendered frame and cannot be
  mistaken for durable observation or lifecycle state.
