# ADR 0076: Flag quiet Codex sessions as possibly stuck

- Status: Accepted
- Date: 2026-08-23

## Decision

Associate each currently SSH-openable world's playback stream with its sole
active Codex session and track it whenever `wt shell` is running, except while
the Live screen is open. Track it when it is `working` and is not compacting.
Mark it `possibly stuck` after 30 seconds with no visible character changes and
no newer Codex lifecycle observation.

`wt shell` already opens one SSH playback connection for every currently
SSH-openable world and continuously applies every connection's output to its
own `vt100` screen.
That work continues in both the Control UI and a directly opened world. The
existing UI loop therefore compares the tracked screens on every iteration
outside the Live screen. The change detector adds no terminal stream, parser,
persistent worker thread, or remote polling loop.

Each Codex observation already identifies its world, tmux session, and pane.
When a world has one active Codex session, change detection associates that
world's parsed screen with the session; it does not focus the pane outside the
Live screen. Live retains ADR 0045's existing one-shot focus worker and remote
pane-selection command for its preview. While Live is open, retain a prior hint
only after that focus succeeds for the current playback stream.

Start a fresh timer when the session becomes trackable. Reset it when the
character grid, its dimensions, the playback stream identity, or the latest
Codex lifecycle observation changes. Pause screen comparison while WT displays
the Live screen, but remove the hint if the lifecycle, eligibility, or playback
connection changes. Discard the timer while running a queued control action or
when the session stops being trackable. The Live screen sizes the shared
playback terminals to fit its card grid, so WT does not interpret changes made
during that view as session progress. After 30 seconds, render `POSSIBLY STUCK`
in yellow when that session is shown in the Live screen.

Compare characters and screen dimensions only. Cursor movement and
styling-only changes do not count as activity.

Do not capture screenshots, run OCR, or match prompt text. Keep `possibly stuck`
as client-only UI state; do not store it or return it from the server API.

## Consequences

WT can implement this without changing Codex and without increasing the number
of terminal streams. Opening the Live screen pauses screen comparison and
preserves a hint only while its lifecycle and stream association remain valid.
Leaving Live deliberately discards the paused baseline and starts a fresh timer;
a dimension change independently resets a timer. Silent work can produce false
positives. A world playback connection can show one pane at a time. When a world
has multiple active Codex sessions, WT does not rotate the shared stream among
them and does not apply this fallback.

One active Codex session in a world does not prove that the world's playback
stream currently displays its pane. Detection outside Live deliberately accepts
that heuristic without focusing the pane. Even after Live validates and selects
the pane, another tmux client can change the shared selection without changing
WT's playback connection. WT can therefore attribute another pane's activity to
the Codex session. This is a known source of false positives and false negatives,
tracked separately in
`docs/todo/make-shell-playback-pane-selection-durable.md`.
