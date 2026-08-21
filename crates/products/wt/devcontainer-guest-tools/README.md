# wt-devcontainer-guest-tools

Programs installed in each devcontainer world for app access.

| Program | Role |
|---------|------|
| `wt-app-shell` | Guest-installed shell script that attaches the Byobu session |
| `wt-app-pane` | Resolve the current app container and enter it over SSH |
| `wt-app-proxy` | Proxy client OpenSSH to the current app container |
| `wt-app-info` | Report the current app SSH target |

Connection flow: [Devcontainer access](../../docs/worlds/devcontainer.md#access).
