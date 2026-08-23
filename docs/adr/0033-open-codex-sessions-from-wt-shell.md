# ADR 0033: Open Codex sessions from `wt shell`

- Status: Accepted
- Date: 2026-08-21

The Codex activity renders one selectable card per observed session location in
a two-column grid.
The grid uses an independent terminal-row scroll offset. Wheel scrolling does
not change selection; keyboard navigation scrolls only enough to reveal the
selected card. Cards retain fixed grid positions and clip at viewport edges.
Identity includes context, session, world, tmux session, and pane. Cards sort by
state, timestamp, and complete identity. Disabled and context-error cards state
why they cannot open.

Before accepting a context snapshot, validate the strict response envelope,
unique identities, nonnegative timestamps, absolute bounded working
directories, exact inventory identity, an existing playback PTY, `wt-host`, and
a numeric tmux pane ID. Reject the complete context snapshot on any violation.

Each playback SSH attempt owns a distinct private OpenSSH control socket for
its lifetime. Opening waits for that exact master connection, then runs the
WT-owned focus helper through it and `CONTEXT.WORLD-direct`. Network fallback
is disabled for the helper, so it cannot establish a second connection when
the playback master is unavailable. A reconnect invalidates results from the
previous socket.

The helper verifies the pane-local session marker, selects the window and pane,
then switches to the existing playback PTY. The active world changes only after
success. Readiness and the control operation share one 15-second deadline and
never replace a playback SSH session.
