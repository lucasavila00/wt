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
forwards arrow keys and all other input unchanged to its Byobu.

All open world views remain live while overlays are open and when another world
is selected. Switching views does not pause or close their Byobu sessions.

`wt shell` owns the overlay and cross-world navigation. Each world's Byobu owns
its sessions and terminal behavior.

The contents and actions of the control menu are deferred.
