# ADR 0076: Flag quiet Codex sessions as possibly stuck

- Status: Accepted
- Date: 2026-08-23

## Decision

Track a session only while the Live view is open, the session is `working` and
not compacting, and WT has verified that the playback stream shows that
session's pane. Mark it `possibly stuck` after 30 seconds with no visible
character changes and no newer Codex lifecycle observation.

`wt shell` already receives each world's terminal output over its persistent SSH
playback connection and applies that output to a `vt100` screen. While the Live
view is open, its existing UI loop compares the visible character grid with the
previous grid for the verified Codex pane and advances a local timer when they
are equal. This uses no background thread and performs no additional remote
polling.

Start a fresh timer after focus succeeds. Reset it when the character grid, its
dimensions, or the latest Codex lifecycle observation changes. Discard it when
the Live view closes, the connection changes, or WT can no longer verify the
pane. After 30 seconds, render `POSSIBLY STUCK` in yellow on that live card.

Compare characters and screen dimensions only. Cursor movement and
styling-only changes do not count as activity.

Do not capture screenshots, run OCR, or match prompt text. Keep `possibly stuck`
as client-only UI state; do not store it or return it from the server API.

## Consequences

WT can implement this without changing Codex, but silent work can produce false
positives. Leaving the eligible Live view discards elapsed quiet time. Do not
apply the fallback when WT cannot identify the displayed pane.
