# ADR 0052: Add `wt shell`

- Status: Proposed; Date: 2026-08-21

## Decision

`wt shell` is a terminal workspace for WT worlds. It adds navigation across
worlds without replacing the Byobu instance inside each world.

It has:

- one full-screen world view per open world, displaying and forwarding terminal
  input to that world's Byobu;
- a control menu for managing worlds and Byobu sessions.

`F5` opens a small black switcher over the active full-screen world view. It
shows directional controls, a fixed Nerd Font terminal icon, the active world
name, and its position among open worlds. The world remains visible behind it.

While the overlay is open, it captures its navigation keys:

- `Left` and `Right` immediately switch the full-screen view between open
  worlds;
- `Up` opens the control UI;
- `F5` closes the overlay, leaving the active world full-screen.

`F6` always closes `wt shell` and is never forwarded to Byobu. While the overlay
is closed, `wt shell` otherwise captures only `F5`; the active world view
forwards keyboard and paste input to its Byobu. Mouse clicks are also forwarded;
mouse wheel events are forwarded and mouse motion is ignored.

`wt shell` keeps one OpenSSH process, local PTY, and terminal buffer per open
world. All remain live in the background. Switching changes only the visible
buffer and input target; it never reconnects or detaches.

`wt shell` owns the overlay and cross-world navigation. Each world's Byobu owns
its sessions and terminal behavior.

The full-screen control UI has a left activity rail. `Tab` switches between
Worlds and Codex sessions. `1` or `F1` opens its command palette. The initial
activities and commands are scaffolding with no actions. `F5` closes it.
