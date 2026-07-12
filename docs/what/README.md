# What WT does

WT creates named, parallel development environments from an existing
devcontainer recipe. Each environment is a world with its own checkout, Docker
state, network, and running devcontainer.

## Concepts

| Concept | Meaning |
|---------|---------|
| Context | A configured WT server |
| World | One isolated development environment |
| Source | An SSH Git repository cloned into the world |
| Recipe | The repository's existing `devcontainer.json` |

## Commands

| Command | Result |
|---------|--------|
| `wt new SOURCE NAME` | Create a world and wait for it to become usable |
| `wt logs NAME` | Replay and follow provisioning output |
| `wt ls` | List worlds across configured contexts |
| `wt rm NAME` | Destroy a world |
| `wt sync` | Update managed OpenSSH aliases |

`context.world` is the stable name. Short names work when globally unique.

## Access

| Alias | Target |
|-------|--------|
| `NAME` | Persistent app session |
| `NAME-dc` | Devcontainer for VS Code, commands, and file transfer |
| `NAME-host` | Guest shell and recovery |

## Requirements and limits

- Ubuntu 24.04 amd64 servers with KVM.
- SSH Git sources: `ssh://...` or `user@host:path`.
- App images derived from Debian or Ubuntu with `apt`.
- Trusted server users; no hostile multi-tenant isolation.
- The stock devcontainer recipe remains the environment contract.
- No KVM emulation fallback.

WT is not a CI system, Git worktree manager, recipe language, or hosted IDE.

Implementation: [How WT works](../how/README.md).
