# CLI (`wt`)

Parent: [architecture](./README.md). Server:
[`wt-server`](../../crates/wt-server/).

## Contract

- Local context: run `wt-server api`.
- SSH context: run `ssh -- HOST wt-server api`.
- Protocol: one JSON request and response over stdio.
- Client never runs libvirt, Docker, or guest provisioning.
- Client never edits the application checkout or main SSH config.

## Config

Path: `~/.wt/config.toml`.

```toml
version = 1

[[contexts]]
name = "local"
kind = "bare_metal_local"

[[contexts]]
name = "lab"
kind = "bare_metal_ssh"
host = "wt-lab"
```

`context.world` is stable. Short names work only when globally unique. Creating
with a short name requires one configured context. Multi-context operations fail
if any context fails.

## Commands

| Command | Result |
|---------|--------|
| `wt new SOURCE NAME [--ref REF]` | Create, sync SSH, print aliases |
| `wt ls` | List and sync |
| `wt rm NAME` | Destroy and sync |
| `wt sync` | Rewrite managed SSH files |

Sources are SSH only: `ssh://...` or `user@host:path`. WT retries only an invalid
Git-key passphrase.

## SSH

- Managed files: `~/.ssh/wt/config`, `~/.ssh/wt/known_hosts`.
- Main config must include `Include ~/.ssh/wt/config` first.
- `ssh NAME`: persistent tmux session in the app container.
- `ssh NAME-host`: guest shell, commands, SCP, VS Code, recovery.
- Qualified aliases always exist. Short aliases require a unique name.
- Login user: `wt`. Checkout: `/workspace`.
- Aliases use `TERM=xterm-256color`.
- Host keys are pinned. Changed identity is never accepted.
- Error worlds have no alias. `wt ls` prints the reconciliation error.
