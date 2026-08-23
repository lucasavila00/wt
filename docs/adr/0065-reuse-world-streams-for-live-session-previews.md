# ADR 0065: Reuse world streams for live session previews

- Status: Accepted; Date: 2026-08-22

## Context

`wt shell` already owns one persistent SSH PTY for every running world. These
sessions remain connected, receive output, and update their terminal parsers
while the control UI is visible.

tmux normally stores the current window on the session and the active pane on
the window. Clients attached to the same session therefore share the selected
window and pane. Independent per-client active panes require tmux's explicit
`active-pane` client flag, which WT does not request. Session groups can also
share windows while retaining different current windows; WT does not attach its
world playback client through a grouped session.

The common WT workflow has one live Codex session in a world. A world can still
report more than one live Codex target, so the UI must not imply that one world
stream can display several panes simultaneously.

## Decision

- Render live-session cards from the existing per-world terminal parser. Do not
  poll `tmux capture-pane` and do not open a second SSH connection for preview
  content.
- Keep every world SSH PTY running in control view. Resize all of them to the
  live grid's card viewport while the activity is visible, then restore the
  full world viewport when leaving it.
- Show two equal-width columns and as many rows as fit in the terminal.
- When exactly one openable Codex target belongs to a world, validate and focus
  its reported tmux pane when the live activity is entered. The existing world
  stream then follows that shared tmux selection.
- Do not auto-focus a world with multiple openable Codex targets. Its cards
  render the same world stream with an explicit warning that the user must open
  one card to choose a pane.
- If automatic focus fails validation, the SSH session is unavailable, or the
  helper fails, keep rendering the world stream and show `World not focused on
  this Codex session`.
- Opening a card retains ADR 0056's strict marker validation and focuses that
  target before switching to the full world view.
- Treat tmux client modes that give WT an independent active pane, and grouped
  session attachments, as unsupported configuration until WT explicitly adopts
  and tests them.

## Consequences

- The normal one-Codex-session-per-world case is continuously live without
  polling delay, extra SSH authentication, or duplicate terminal parsers.
- Entering the activity can change the active pane seen by other clients
  attached to the same tmux session because active pane selection is shared by
  default.
- Two cards from one world cannot show different live panes simultaneously.
  They remain honest and navigable rather than presenting duplicated content as
  independent streams.
- Preview dimensions affect the shared world PTY while the activity is visible;
  leaving the activity restores the full terminal dimensions.

## References

- [tmux terms and object relationships](https://github.com/tmux/tmux/wiki/Getting-Started#summary-of-terms)
- [tmux `attach-session` client flags](https://man.openbsd.org/tmux#attach-session)
- [tmux FAQ: separate current windows through grouped sessions](https://github.com/tmux/tmux/wiki/FAQ#how-do-i-attach-the-same-session-to-multiple-clients-but-with-a-different-current-window-like-screen--x)
