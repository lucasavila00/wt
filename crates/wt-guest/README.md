# wt-guest

Programs installed in each world for app access.

| Program | Role |
|---------|------|
| `wt-app-shell` | Guest-installed shell script that attaches the Byobu session |
| `wt-app-pane` | Resolve the current app container and enter it over SSH |
| `wt-app-proxy` | Proxy client OpenSSH to the current app container |
| `wt-app-info` | Report the current app SSH target |

Connection flow: [SSH inventory](../../docs/how/cli.md#ssh-inventory).
