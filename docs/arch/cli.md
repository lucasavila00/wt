# CLI and SSH

## Contexts

`~/.wt/config.toml` names local and OpenSSH servers:

```toml
version = 1

[[contexts]]
name = "local"
kind = "bare_metal_local"

[[contexts]]
name = "lab"
kind = "bare_metal_ssh"
host = "wt-server"
```

- Local: `wt-server api`.
- Remote: `ssh -- HOST wt-server api`.
- Stable world name: `context.world`.
- Short names require global uniqueness.
- Multi-context operations fail if any context fails.

## Commands

| Command | Result |
|---------|--------|
| `wt new SOURCE NAME` | Create, follow logs, then sync SSH inventory |
| `wt logs NAME` | Replay and follow provisioning logs |
| `wt ls` | List worlds and sync inventory |
| `wt rm NAME` | Destroy a world and sync inventory |
| `wt sync` | Rewrite managed SSH files |

Git sources must use `ssh://...` or `user@host:path`.

`wt new` follows a detached job after acknowledgement. Ctrl-C stops following,
not provisioning. Resume with `wt logs`. If transport fails before
acknowledgement, check `wt ls` or `wt logs`.

## Managed SSH

Place this before every `Host` block in `~/.ssh/config`:

```sshconfig
Include ~/.ssh/wt/config
```

`wt sync` owns `~/.ssh/wt/config` and `~/.ssh/wt/known_hosts`. It does not edit
the main SSH config.

| Alias | Behavior |
|-------|----------|
| `NAME` | Attach to the guest-hosted tmux or Byobu app session |
| `NAME-dc` | Direct app SSH for VS Code, commands, SFTP, and forwarding |
| `NAME-host` | Direct guest SSH for commands and recovery |

Qualified aliases always exist. Short aliases exist for globally unique names.
Host keys are pinned. Non-running worlds have no aliases.

Parent: [architecture](./README.md).
