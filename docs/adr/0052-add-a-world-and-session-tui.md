# ADR 0052: Add `wt shell`

- Status: Proposed; Date: 2026-08-21

## Decision

`wt shell` is a terminal workspace for WT worlds. It adds navigation across
worlds without replacing the Byobu instance inside each world.

It has:

- one full-screen world view per open world, displaying and forwarding terminal
  input to that world's Byobu;
- a control menu for managing worlds and Byobu sessions.

`F5` opens a small black switcher over the active full-screen world view. The
switcher shows the available directional controls; the world remains visible
behind it.

While the overlay is open, it captures its navigation keys:

- `Left` and `Right` immediately switch the full-screen view between open
  worlds;
- `Up` opens the control UI;
- `F5` closes the overlay, leaving the active world full-screen.

`F6` always closes `wt shell` and is never forwarded to Byobu. While the overlay
is closed, `wt shell` otherwise captures only `F5`; the active world view
forwards keyboard and paste input to its Byobu. Mouse clicks are also forwarded;
other mouse events are ignored.

`wt shell` keeps one OpenSSH process, local PTY, and terminal buffer per open
world. All remain live in the background. Switching changes only the visible
buffer and input target; it never reconnects or detaches.

`wt shell` owns the overlay and cross-world navigation. Each world's Byobu owns
its sessions and terminal behavior.

The initial control UI contains only `CONTORL UI`; its contents and actions are
deferred. `F5` closes it.
