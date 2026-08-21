# Terminal workspace

`wt shell` opens one SSH process, local PTY, and terminal buffer for every
accessible world. Background worlds remain connected and continue processing
output. The top row is a WT navbar; the active world's Byobu uses the remaining
terminal rows.

The dim navbar shows the active world and its position in the world list. `F5`
enables the navbar controls, `Left` and `Right` change worlds, and `Up` opens the
Control UI. Press `F5` again to return keyboard control to the world. `F6`
closes `wt shell` from every view. Other keyboard input is forwarded to the
active world, including while the navbar is enabled.

Paste, terminal resize, application cursor mode, bracketed paste, mouse button
press and release, and vertical and horizontal wheel events are supported.
Mouse input is forwarded only when the application has enabled a terminal
mouse protocol. The navbar row is WT-owned; mouse coordinates in the world view
are translated to the guest PTY, and mouse input on the navbar is ignored.

OSC 52 clipboard writes from the visible world are relayed to the workstation
terminal. Writes from background worlds are ignored. Clipboard-read queries
are deliberately not relayed; visible world code can set, but cannot retrieve,
the workstation clipboard through `wt shell`.

The Control UI's Codex activity is a card snapshot loaded when `wt shell`
starts. `Up` and `Down` select cards, the mouse wheel scrolls them, and `Enter`
or left click opens the selected live Codex pane. Opening uses a short control
SSH connection; it does not replace any world's playback connection.

Cards show activity, context, world, Byobu target, session, working directory,
and report age. Inactive and rollout-only cards explain why they cannot open.
Malformed context data and failed pane checks remain visible as exact errors;
WT does not guess another world or pane. Restart `wt shell` to refresh the
snapshot.

Opening requires worlds provisioned by a WT version containing this feature.
After upgrading WT, recreate older worlds so their relay records pane markers
and their focus helper is current.

Known terminal-compatibility gaps are TODOs to fix:

- TODO: Forward mouse drag, position, and hover/motion events when the active
  application requests button-motion or any-motion reporting.
- TODO: Forward terminal focus gained and lost events when the application
  enables focus reporting.
- TODO: Forward paste input while the F5 navbar is enabled, just as unhandled
  keyboard input is forwarded.
- TODO: Propagate guest window-title changes and audible or visual bells to the
  workstation terminal.
