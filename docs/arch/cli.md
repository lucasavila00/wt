# CLI (`wt`)

Parent: [architecture](./README.md). Helper: [`wt-server`](../../crates/wt-server/).

## Responsibilities

| Does | Does not |
|------|----------|
| Dispatch to local or OpenSSH `wt-server api` | Run libvirt or Docker itself |
| Send one JSON request over stdin | Add a public network protocol |
| Parse one JSON response from stdout | Provision guests over SSH |
| Create, list, and remove worlds | Export guest checkouts to the client |
| Project guest inventory into managed OpenSSH files | Edit the user's main SSH config |

## Client contexts

The strict client config is `~/.wt/config.toml`:

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

A local context executes `wt-server api`. An SSH context executes
`ssh -- <host> wt-server api`, using the user's existing OpenSSH configuration
and authentication. Context names contain lowercase letters, digits, and
internal hyphens. SSH hosts must be non-empty and must not begin with `-`.

`context.world` is the stable qualified name. A short world name resolves only
when it is globally unique. Short-name creation requires exactly one configured
context. Aggregate list and sync operations fail if any context is unavailable.

## Commands

| Command | Behavior |
|---------|----------|
| `wt new <source> <name> [--ref <ref>]` | Select a context, prompt locally for the server Git identity's passphrase, clone, start the devcontainer, sync access, and print status and aliases |
| `wt ls` | List worlds across all contexts and refresh managed SSH inventory |
| `wt rm <name>` | Resolve and destroy a world, then refresh managed SSH inventory |
| `wt sync` | Atomically rewrite managed SSH config and known-hosts files from all running worlds |

Git sources must use `ssh://` or `user@host:path`. With no `--ref`, Git uses the
remote default branch. A supplied ref may identify an existing branch, tag, or
commit. The client never edits the application repository or mounts its checkout
on the client host.

## Guest access

- The guest login is the fixed non-root user `wt`; the checkout is `/workspace`.
- The base alias always attaches to one shared persistent tmux session. Every
  tmux window and pane enters the primary app container; SSH disconnects do not
  terminate its shells or processes.
- Every world has unique SSH host keys. `wt-libvirt` retrieves their public parts
  through the QEMU guest agent, and `wt-server` persists them with the endpoint.
- `wt sync` writes `~/.ssh/wt/config` and `~/.ssh/wt/known_hosts`. The user adds
  `Include ~/.ssh/wt/config` at the beginning of the main OpenSSH config.
- After syncing, users enter a world with stock OpenSSH: `ssh <name>` or
  `ssh <context>.<name>`.
- Qualified aliases always exist. Short aliases exist only for globally unique
  names. The base alias enters the persistent app session; the `-host` alias
  provides unrestricted guest SSH for commands, SCP, VS Code Remote SSH, and
  recovery when the app-session path cannot start.
- Both aliases enforce the world's recorded host-key identity. A changed DHCP
  address may be reconciled, but a different host key is never accepted silently.
- SSH readiness, clone, checkout, and `devcontainer up` must all succeed before a
  world becomes `Running`.
