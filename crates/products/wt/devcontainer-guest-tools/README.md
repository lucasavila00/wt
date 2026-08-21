# wt-devcontainer-guest-tools

Programs installed in each devcontainer world for app access.

| Program | Role |
|---------|------|
| `wt-app-shell` | Guest-installed shell script that attaches the Byobu session |
| `wt-devcontainer-pane` | Resolve the current app container and enter it over SSH |
| `wt-devcontainer-ssh-proxy` | Proxy client OpenSSH to the current app container |
| `wt-devcontainer-info` | Report the current app SSH target |

Connection flow: [Devcontainer access](../../docs/worlds/devcontainer.md#access).
