# ADR 0076: Flag quiet Codex sessions as possibly stuck

- Status: Accepted
- Date: 2026-08-23

## Decision

Track the Codex session displayed by each world's playback stream for as long as
`wt shell` remains in its Control UI, regardless of which Control screen is
selected. Track it when it is `working`, is not compacting, and is the only
active Codex session in that world. Mark it `possibly stuck` after 30 seconds
with no visible character changes and no newer Codex lifecycle observation.

`wt shell` already opens one persistent SSH playback connection for every world
and continuously applies every connection's output to its own `vt100` screen.
That work does not start or stop when the user changes Control screens. The
existing UI loop therefore compares the tracked screens on every iteration;
it does not create another stream, background thread, or remote polling loop.

Each Codex observation already identifies its world, tmux session, and pane. WT
uses that identity when it selects the pane on the existing world playback
connection, so the parsed screen belongs to the selected Codex session. Start a
fresh timer when WT selects that pane. Reset it when the character grid, its
dimensions, or the latest Codex lifecycle observation changes. Discard it when
WT leaves the Control UI, temporarily stops screen observation while running a
queued control action, replaces the playback connection, or the session stops
being trackable. After 30 seconds, render `POSSIBLY STUCK` in yellow when that
session is shown in the Live screen.

Compare characters and screen dimensions only. Cursor movement and
styling-only changes do not count as activity.

Do not capture screenshots, run OCR, or match prompt text. Keep `possibly stuck`
as client-only UI state; do not store it or return it from the server API.

## Consequences

WT can implement this without changing Codex and without increasing the number
of terminal streams. Moving between Control screens does not lose elapsed quiet
time. Silent work can produce false positives. A world playback connection can
show one pane at a time. When a world has multiple active Codex sessions, WT
does not rotate the shared stream among them and does not apply this fallback.
