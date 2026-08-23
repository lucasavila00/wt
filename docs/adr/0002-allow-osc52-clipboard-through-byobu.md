# ADR 0002: Allow OSC 52 clipboard writes through Byobu

- Status: Accepted
- Date: 2026-07-17

The golden-image tmux profile enables clipboard handling and passthrough for
visible panes and declares Ghostty clipboard support explicitly:

```tmux
set-option -s set-clipboard on
set-option -g allow-passthrough on
set-option -as terminal-features ',xterm-ghostty:clipboard'
```

Passthrough is `on`, not `all`, so invisible panes cannot bypass tmux. Code in
a visible world pane can therefore change the workstation clipboard.
