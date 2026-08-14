# ADR 0008: Allow OSC 52 clipboard writes through Byobu

- Status: Accepted
- Date: 2026-07-17

## Context

Devcontainer applications could not write to the workstation clipboard through
WT's guest Byobu session. tmux blocks raw OSC 52 unless clipboard handling is
enabled and blocks wrapped sequences unless passthrough is enabled.

## Decision

The shared tmux profile contains:

```tmux
set-option -s set-clipboard on
set-option -g allow-passthrough on
set-option -as terminal-features ',xterm-ghostty:clipboard'
```

Use passthrough mode `on`, not `all`, so invisible panes cannot bypass tmux.
Declare Ghostty explicitly instead of relying on tmux's broad `xterm*` default.
The guest owns this configuration; the devcontainer does not need tmux access.

Existing tmux servers read the change only after restart. Restarting destroys
their panes, so recreate or restart them only when no work needs to survive.

## Verification

- Send raw OSC 52 from a visible devcontainer pane and verify the clipboard.
- Repeat with a tmux-wrapped sequence.
- Verify an invisible pane cannot use passthrough.

## Consequences

Code in the visible pane can change the workstation clipboard. This is an
explicit capability of trusted world code. Raw OSC 52 also updates tmux's own
buffer.

Enabling only clipboard handling or only passthrough was rejected because each
supports only one form. Configuring the devcontainer was rejected because the
tmux server runs in the guest.
