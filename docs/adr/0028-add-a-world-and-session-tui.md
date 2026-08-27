# ADR 0028: Add `wt shell`

- Status: Accepted; Date: 2026-08-21

## Decision

`wt shell` is a terminal workspace for WT worlds. It adds navigation across
worlds without replacing the Byobu instance inside each world.

It has:

- one full-screen world view per open world, displaying and forwarding terminal
  input to that world's Byobu;
- a control menu for managing worlds and Byobu sessions.

The active full-screen world view has a dim control bar showing the active
world and command hints. `F5` or the clickable world target opens the full
control UI directly.

`F6` always closes `wt shell` and is never forwarded to Byobu. While a world is
open, `wt shell` otherwise captures only `F5`; the active world view
forwards keyboard and paste input to its Byobu. Mouse clicks are also forwarded;
mouse wheel events are forwarded and mouse motion is ignored.

`wt shell` keeps one OpenSSH process, local PTY, and terminal buffer per open
world. All remain live in the background. Switching changes only the visible
buffer and input target; it never reconnects or detaches.

`wt shell` owns the control UI and cross-world navigation. Each world's Byobu
owns its sessions and terminal behavior.

The full-screen control UI has a left activity rail. `Tab` switches between
server-observed Byobu panes and Worlds. `1` or `F1` opens its command palette.
`F5` opens the active world.
