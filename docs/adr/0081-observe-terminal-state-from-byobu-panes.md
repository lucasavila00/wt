# ADR 0081: Observe terminal state from Byobu panes

- Status: Accepted
- Date: 2026-08-26

## Context

WT needs shared live previews of interactive Codex panes without tracking
Codex's application lifecycle. A world's playback PTY shows only its selected
Byobu pane, so it cannot supply independent previews of every Codex pane.

## Decision

The guest relay observes panes whose foreground process is `codex` and sends
bounded normalized frames through its authenticated server connection. Each
observation identifies `(world, tmux session, pane ID)` and includes freshness,
screen fingerprint, current directory, optional Git branch, and a `PaneRender`
containing styled terminal cells and the Byobu window index and name.

`wts` keeps each world's complete latest snapshot only in bounded memory and
serves it with owner-scoped world metadata. No pane observation is registry
state. Server restart, world stop, or grant revocation clears the affected
observations. Per-world operation locks and run generations prevent in-flight
reports from restoring stale observations across a stop and restart.

Each Live preview renders its exact observed frame as inert cells. Opening it
verifies and selects that pane through the existing playback connection's SSH
control master, then shows the shared Byobu session. Collecting observations
does not switch panes or create playback connections.

Labels use the familiar Byobu window name, such as `Codex · window “codex”`,
with the window index available for ordering and disambiguation. Names and
indexes are presentation data; the exact pane target remains the identity.

Stale or missing observations mean unavailable terminal data, not application
exit. No Codex lifecycle hook, thread ID, or rollout scan supplies live state.
Authentication and per-world session storage are separate infrastructure.

## Consequences

- Multiple Codex panes have independent previews, regardless of client playback.
- Window renames appear on the next report without changing pane identity.
- Observations disappear completely on server restart and return on new reports.
- WT depends on terminal semantics and represents uncertainty explicitly;
  Codex behind a foreground wrapper is not observed.
