# OSC 52 clipboard is blocked by WT Byobu

## Bug

Clipboard writes from apps inside a WT workspace do not reach the local terminal.

Example path:

```text
Ghostty -> Byobu/tmux -> SSH -> WT VM -> devcontainer -> Diffo
```

Diffo sends OSC 52. Its success toast appears. The local clipboard does not change.

## Evidence

Inside the devcontainer:

```text
TERM=screen-256color
SSH_TTY=/dev/pts/1
TMUX is unset
STY is unset
tmux is not installed
```

The Byobu process is outside the container. The app cannot call its tmux server. Raw OSC 52 and passthrough sequences are blocked before they reach Ghostty.

## Expected

OSC 52 emitted inside the WT app shell reaches Ghostty and updates the local clipboard.

## Fix

Configure the Byobu/tmux instance created by WT:

```tmux
set -s set-clipboard on
set -g allow-passthrough on
set -as terminal-features ',xterm-ghostty:clipboard'
```

See [ADR 0008](../adr/0008-allow-osc52-clipboard-through-byobu.md).

Apply this during WT guest provisioning. Fully restart the tmux server after changing the configuration:

```bash
byobu kill-server
```

## Regression test

From inside a WT devcontainer, emit OSC 52 containing a known value. Read or paste the local Ghostty clipboard and verify the exact value arrived.

Test both:

- Raw OSC 52.
- OSC 52 wrapped in tmux passthrough DCS.

## Notes

Ghostty supports OSC 52 clipboard writes. SSH forwards terminal output unchanged. The blocking layer is the WT-managed Byobu/tmux session.
