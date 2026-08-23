# ADR 0056: Open Codex sessions from `wt shell`

- Status: Proposed
- Date: 2026-08-21

The Codex activity renders one selectable card per observed session location.
Identity includes context, session, world, tmux session, and pane. Cards sort by
state, timestamp, and complete identity. Disabled and context-error cards state
why they cannot open.

Before accepting a context snapshot, validate the strict response envelope,
unique identities, nonnegative timestamps, absolute bounded working
directories, exact inventory identity, an existing playback PTY, `wt-host`, and
a numeric tmux pane ID. Reject the complete context snapshot on any violation.

Each playback SSH connection owns a private OpenSSH control socket for the
lifetime of `wt shell`. Opening runs the WT-owned focus helper through that
connection and `CONTEXT.WORLD-direct`, verifies the pane-local session marker,
selects the window and pane, then switches to the existing playback PTY. The
active world changes only after success. The short control operation has a
15-second deadline and never replaces a playback SSH session.
