# Client and SSH

The client reads `~/.wt/config.toml`:

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

A local context runs `wt-server api`. An SSH context runs it through the named
OpenSSH host. Use `CONTEXT.WORLD` explicitly; a short name works only when it
is unique across every context. World names cannot end in `-direct`, which is
reserved for managed SSH.

## Commands

| Command | Result |
|---------|--------|
| `wt new` | Interactively create a world and open its Byobu workspace |
| `wt ls` | List status, resources, and disk use |
| `wt start NAME` | Start the existing guest and disk |
| `wt stop NAME` | Shut down the guest and keep its disk |
| `wt ssh NAME` | Sync managed aliases and connect to Byobu |
| `wt shell` | Open all accessible worlds in one [terminal workspace](./shell.md) |
| `wt rm NAME` | Destroy the world |
| `wt sync` | Rewrite managed SSH inventory |

Each world has a writable qcow2 overlay on the server. A running disk display
such as `1.5G/32G` reports allocated and maximum size; a stopped world reports
allocated size only. `wt stop` keeps the overlay and `wt rm` deletes it.

New worlds require at least one valid regular `~/.ssh/*.pub` file. Private keys
are never sent to the server. Every world receives the workstation's global
Git `user.name` and `user.email`; creation stops before contacting the server
when either value is missing.

Provisioning is not resumable. Remove a failed world with `wt rm`, then create
its replacement from the golden image.

## Managed SSH

When `~/.ssh/config` does not exist, `wt sync` creates it with:

```sshconfig
Include ~/.ssh/wt/config
```

WT never modifies an existing main configuration. The include must be outside
any `Host` or `Match` block. WT owns `~/.ssh/wt/config` and
`~/.ssh/wt/known_hosts`, pins guest host keys, and uses a jump host for remote
contexts.

`CONTEXT.WORLD` opens Byobu. `CONTEXT.WORLD-direct` opens a normal guest shell
or runs a command. Managed aliases do not enable SSH-agent forwarding.

World-changing commands synchronize automatically. Run `wt sync` on another
workstation after changing worlds elsewhere.
