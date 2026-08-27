# ADR 0081: Observe terminal state from Byobu panes

- Status: Accepted
- Date: 2026-08-26

## Decision

Rendered Byobu panes are the sole source of live terminal state. The guest
observer identifies each pane's foreground process through tmux and sends
bounded normalized observations for `codex` panes through the authenticated
path; `wts` persists and serves them. Observations identify world, tmux
session, pane, screen-derived fingerprint, timestamps, and current working
directory. When that directory has a `.git` folder, they also contain its
checked-out branch. They contain no Codex IDs, hook events, lifecycle, or raw
screen content.

The shell renders these server observations as Codex panes. Stale or missing
data stays stale or unavailable; it never implies an application exit. Live
previews use the shell's existing per-world SSH/PTY parsers. Opening a preview
verifies and selects its matching Codex pane through the playback connection's
SSH control master, then shows the world. The shell does not switch panes to
create observation state or create another playback connection for previews.

This is one incompatible cutover: delete every Codex lifecycle hook, report,
protocol, persistence record, local liveness/checkout tracker, and related UI.
Retain only shared authentication and startup history synchronization; neither
may update live state.

## Migration

Deploy the observer, server query, and shell UI together after `make clear`.
There is no migration, backfill, or compatibility path.

## Consequences

- Live state is shared for every observed Codex pane, independent of client
  playback.
- WT depends on terminal semantics and represents uncertainty explicitly.
- Codex upgrades cannot strand WT lifecycle state.
- Codex behind a foreground wrapper is not observed.
