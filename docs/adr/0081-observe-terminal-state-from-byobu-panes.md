# ADR 0081: Observe terminal state from Byobu panes

- Status: Accepted
- Date: 2026-08-26

## Decision

Rendered Byobu panes are the sole source of live terminal state. The guest
observer identifies each pane's foreground process through tmux and sends
bounded normalized observations for `codex` panes through the authenticated
path. Observations identify world, tmux session, and pane. The registry
persists only their fingerprint and timestamps; `wts` retains the latest
normalized terminal frame for each observation in memory, without logs or
history. Frames are owner-scoped and disappear on server restart or when their
pane observation disappears. They contain no Codex IDs, hook events,
lifecycle, or checkout state.

The shell renders these server observations as Codex panes. Stale or missing
data stays stale or unavailable; it never implies an application exit. Each
Live preview renders only the frame for its exact `(world, tmux session,
pane)` observation, never a world SSH/PTY playback screen. A frame is an inert
grid of styled terminal cells, not ANSI replay, scrollback, or a raster image.
Opening a preview verifies and selects its matching Codex pane through the
playback connection's SSH control master, then shows the world. The shell does
not switch panes to create observation state or create another playback
connection for previews.

This is one incompatible cutover: delete every Codex lifecycle hook, report,
protocol, persistence record, local liveness/checkout tracker, and related UI.
Retain only shared authentication and startup history synchronization; neither
may update live state.

## Consequences

- Live state and its preview are shared for every observed Codex pane,
  independent of client playback.
- Multiple Codex panes in one world render independently and cannot display
  another pane or a non-Codex tab.
- WT depends on terminal semantics and represents uncertainty explicitly.
- Codex upgrades cannot strand WT lifecycle state.
- Codex behind a foreground wrapper is not observed.
