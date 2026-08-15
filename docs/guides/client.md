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
OpenSSH host. Each invocation contacts one context. With one configured context
it is selected automatically; with more than one, use a global context option:

```text
wt --ctx lab ls
wt --ctx lab start my-world
```

The arguments after `--ctx` are sent to that server. World arguments are
unqualified names within the selected context.

World names cannot end in `-host` or `-vs`; managed SSH reserves those suffixes.

## Commands

| Command | Kinds | Result |
|---------|-------|--------|
| `wt new` | devcontainer | Interactively create and enter setup |
| `wt new host` | host | Read user-data from the interactive input stream, prepare the guest, then run it in Byobu |
| `wt ls` | retained | List kind, status, resources, and repository on the selected server |
| `wt start NAME` | retained | Start the existing guest and disk |
| `wt code NAME` | devcontainer | Open the live app workspace in VS Code |
| `wt rm NAME` | retained | Destroy the world |
| `wt sync` | retained | Rewrite managed SSH inventory |

There is no `wt stop`. Shut a guest down from inside it, then use `wt start` to
resume it.

New retained worlds require at least one valid regular `~/.ssh/*.pub` file.
Private keys are never sent to the server.

Host setup uses the workstation SSH agent. Start one and load the key needed by
the recipe before `wt new host`; keep that first Byobu connection open while
the recipe uses it. At the user-data prompt, paste the cloud-init document and
finish it with end-of-file.

Devcontainer creation also requires an SSH-form Git source and global Git
`user.name` and `user.email`. Creation stops before a lifecycle request when
either author value is missing. `wt code` requires the local `code` CLI and VS
Code Remote-SSH extension.

## Managed SSH

When `~/.ssh/config` does not exist, `wt sync` creates it with:

```sshconfig
Include ~/.ssh/wt/config
```

WT never modifies an existing main configuration. Other global includes may
precede the WT include; it must only be outside any `Host` or `Match` block. WT
reports the manual change required when it is missing or scoped. `wt sync`
updates only the selected context below `~/.ssh/wt/contexts/`. It pins host
keys. Remote contexts use their configured server as a jump host to the guest's
private address.

Devcontainer aliases are documented in
[Devcontainer worlds](../worlds/devcontainer.md#access). For hosts,
`CONTEXT.NAME` attaches to Byobu and `CONTEXT.NAME-vs` is direct guest SSH. Both
host aliases forward the workstation's SSH agent. Devcontainer aliases do not.

Only context-qualified SSH aliases are generated. `wt new`, `wt ls`, `wt
start`, and `wt rm` synchronize the selected context automatically. Run `wt
sync` on another workstation after changing worlds elsewhere.
