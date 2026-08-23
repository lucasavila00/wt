# Terminal workspace

`wt shell` opens one SSH process, local PTY, and terminal buffer for every
accessible world. Background worlds remain connected and continue processing
output. The top row is a WT navbar; the active world's Byobu uses the remaining
terminal rows.

The world list and Codex sessions refresh independently in the background. Each
worker starts its next refresh five seconds after the previous one finishes.
Worlds created elsewhere are connected automatically, and removed worlds
disappear. A world refresh that cannot list every configured context leaves the
last complete list in place.

The dim navbar shows the active world and its position in the world list. `F5`
or a click on the navbar enables its controls. `Left` and `Right`, or the
clickable arrows beside the active world, change worlds, and `Up`, the ` WT`
brand, the world label, or the `↑ ctrl` label opens the Control UI. Press `F5`
or click elsewhere on the enabled navbar to return
keyboard control to the world. Clickable navbar text is bold. `F6` closes `wt
shell` from every view. While the navbar is enabled, `F1` or `1` opens the
command palette over the active Byobu session; it captures input until closed
or a command starts its modal. Other keyboard input is forwarded to the active
world.

If an SSH process exits, its last terminal contents remain visible and a red
bottom bar reports that the session ended. Press `Space` to reconnect that
world. Other terminal input is held until the connection is restored.

`Shift+F5` disables WT's `F5` override so `F5` reaches Byobu. A red top bar
shows that the override is disabled. Press `Shift+F5` or click the navbar to
restore it.

Paste, terminal resize, application cursor mode, bracketed paste, mouse button
press and release, and vertical and horizontal wheel events are supported.
Mouse input is forwarded only when the application has enabled a terminal
mouse protocol. The navbar row is WT-owned; mouse coordinates in the world view
are translated to the guest PTY.

OSC 52 clipboard writes from the visible world are relayed to the workstation
terminal. Writes from background worlds are ignored. Clipboard-read queries
are deliberately not relayed; visible world code can set, but cannot retrieve,
the workstation clipboard through `wt shell`.

`wt shell` opens in the Control UI with Codex sessions selected. `Tab` cycles
through Codex sessions, Worlds, and the experimental live-session activity.
`F5` opens the active world when one is available.

The Worlds activity shows cards with each world's status, resources, and
actionable details. The Codex activity refreshes session cards in the
background. All activities use a two-column card grid. Arrow keys select cards,
the mouse wheel scrolls them by row, and `Enter` or left click opens the selected
world or live Codex pane. Opening a Codex pane uses a short control SSH
connection; it does not replace any world's playback connection.

World creation and deletion continue in the background after their forms are
confirmed. Both show the same progress notification in the top-right corner;
click `×` to hide the notification without cancelling the operation. Other
shell navigation remains available while either operation runs.

Cards show activity, title, repository, branch, working directory, context,
world, Byobu target, session, and report age. Inactive and saved-session cards
explain why they cannot open.
Malformed context data remains visible as an exact error. Failed context
queries leave existing cards intact and show the query error in the Codex title
beside the last successful update time. Failed pane-open checks leave existing
cards intact and show a persistent, sanitized notification with a bold,
clickable `Retry` action; `Enter` retries and `Esc` dismisses it. WT does not
guess another world or pane. The Worlds and Codex titles show when their latest
snapshot was applied in UTC, or `Updating…` before the first snapshot arrives.

Opening requires worlds provisioned by a WT version containing this feature.
After upgrading WT, recreate older worlds so their relay records pane markers
and their focus helper is current.

An unknown observation includes its raw Codex session-start source when one was
reported, such as `unknown(compact)`.

The experimental live-session activity uses the same observed sessions but
shows each session's state and report age around the persistent live terminal
stream WT already maintains for its world. Previews use a two-column grid with
as many rows as fit in the terminal. WT temporarily resizes world playback
terminals to the smaller preview viewport while this activity is visible and
restores the full-screen size when it is left. Arrow keys, the mouse wheel,
`Enter`, and clicking navigate or open the previews. When a world has one live
Codex session, WT focuses that session's reported pane on entry. Cards for
multiple live sessions in one world share its one stream and show a warning;
open a card to choose the pane in the full world view.

Known terminal-compatibility gaps are TODOs to fix:

- TODO: Forward mouse drag, position, and hover/motion events when the active
  application requests button-motion or any-motion reporting.
- TODO: Forward terminal focus gained and lost events when the application
  enables focus reporting.
- TODO: Forward paste input while the F5 navbar is enabled, just as unhandled
  keyboard input is forwarded.
- TODO: Propagate guest window-title changes and audible or visual bells to the
  workstation terminal.
