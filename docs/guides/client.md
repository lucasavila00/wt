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
OpenSSH host. Use `CONTEXT.WORLD` explicitly; a short name works only when
unique across every context.

World names cannot end in `-host` or `-vs`; managed SSH reserves those suffixes.

## Commands

| Command | Kinds | Result |
|---------|-------|--------|
| `wt new` | devcontainer | Interactively create and enter setup |
| `wt new host FILE` | host | Create from exact cloud-init user-data |
| `wt ls` | retained | List kind, status, resources, and repository when present |
| `wt start NAME` | retained | Start the existing guest and disk |
| `wt code NAME` | devcontainer | Open the live app workspace in VS Code |
| `wt rm NAME` | retained | Destroy the world |
| `wt sync` | retained | Rewrite managed SSH inventory |

There is no `wt stop`. Shut a guest down from inside it, then use `wt start` to
resume it.

New retained worlds require at least one valid regular `~/.ssh/*.pub` file.
Private keys are never sent to the server.

Devcontainer creation also requires an SSH-form Git source and global Git
`user.name` and `user.email`. The client stops before contacting the server when
either author value is missing. `wt code` requires the local `code` CLI and VS
Code Remote-SSH extension.

## Managed SSH

Add this before other `Host` blocks in `~/.ssh/config`:

```sshconfig
Include ~/.ssh/wt/config
```

`wt sync` owns `~/.ssh/wt/config` and `~/.ssh/wt/known_hosts`. It pins host
keys. Remote contexts use their configured server as a jump host to the guest's
private address.

Devcontainer aliases are documented in
[Devcontainer worlds](../worlds/devcontainer.md#access). For hosts,
`CONTEXT.NAME` attaches to Byobu and `CONTEXT.NAME-vs` is direct guest SSH. Both
host aliases forward the workstation's SSH agent. Devcontainer aliases do not.

`wt new`, `wt ls`, `wt start`, and `wt rm` synchronize automatically. Run
`wt sync` on another workstation after changing worlds elsewhere.
