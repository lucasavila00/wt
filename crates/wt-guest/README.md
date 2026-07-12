# wt-guest

Programs installed in each world for app access.

| Program | Role |
|---------|------|
| `wt-app-shell` | Attach the configured tmux or Byobu session |
| `wt-app-pane` | Resolve the current app container and enter it over SSH |
| `wt-app-proxy` | Proxy client OpenSSH to the current app container |
| `wt-app-info` | Report the current app SSH target |

Connection flow: [CLI and SSH](../../docs/arch/cli.md#managed-ssh).
