# Terminal workspace

`wt shell` opens one SSH process, local PTY, and terminal buffer for every
accessible world. Background worlds remain connected and continue processing
output. The top row is a WT control bar; the active world's Byobu uses the remaining
terminal rows.

The world list and Codex Byobu panes refresh independently in the background. Each
worker starts its next refresh five seconds after the previous one finishes.
Worlds created elsewhere are connected automatically, and removed worlds
disappear. A world refresh that cannot list every configured context leaves the
last complete list in place.

The dim control bar shows the active world and its position in the world list, along
with command hints. `F5`, the ` WT` brand, the world label, or the `F5: dashboard`
label opens the Control UI. Clickable control-bar text is bold. `F6` closes `wt
shell` from every view. Other keyboard input is forwarded to the active world.

If an SSH process exits, its last terminal contents remain visible and a red
bottom bar reports that the session ended. Press `Space` to reconnect that
world. Other terminal input is held until the connection is restored.

Paste, terminal resize, application cursor mode, bracketed paste, mouse button
press and release, and vertical and horizontal wheel events are supported.
Mouse input is forwarded only when the application has enabled a terminal
mouse protocol. The control-bar row is WT-owned; mouse coordinates in the world view
are translated to the guest PTY.

OSC 52 clipboard writes from the visible world are relayed to the workstation
terminal. Writes from background worlds are ignored. Clipboard-read queries
are deliberately not relayed; visible world code can set, but cannot retrieve,
the workstation clipboard through `wt shell`.

`wt shell` opens in the Control UI with live Codex screens selected. `Tab` cycles
through Live, Codex, and Worlds.
`F5` opens the active world when one is available. The active activity and its
refresh status are shown in the footer. Press `2` or `F2` to toggle the shortcut
help menu; `Esc` closes it.

The Worlds activity shows cards with each world's status, resources, and
actionable details. The Codex activity refreshes tracker-derived cards only for
Byobu panes whose foreground process is `codex`. Live shows those same cards
with a preview rendered from the persistent SSH/PTY playback connection already
owned by each world. `Enter` or left click on a Live preview verifies and
selects its matching Codex pane in Byobu, then opens that world. The focus
request reuses the playback connection's SSH control master.

World creation and deletion continue in the background after their forms are
confirmed. Both show the same progress notification in the top-right corner;
click `×` to hide the notification without cancelling the operation. Other
shell navigation remains available while either operation runs.

Cards show the observed pane, world, context, change age, and freshness. A
recent screen change is `CHANGING`; an unchanged pane is `STATIC`. The Codex
panel identifies the foreground process only; it does not infer a Codex session
lifecycle, working directory, or application liveness. Failed pane-context
queries leave existing cards intact and show the query error in red beside the
last successful update time. The Worlds and Codex footer labels show when their
latest snapshot was applied in UTC, or `Updating…` before the first snapshot
arrives.

Known terminal-compatibility gaps are TODOs to fix:

- TODO: Forward mouse drag, position, and hover/motion events when the active
  application requests button-motion or any-motion reporting.
- TODO: Forward terminal focus gained and lost events when the application
  enables focus reporting.
- TODO: Propagate guest window-title changes and audible or visual bells to the
  workstation terminal.
