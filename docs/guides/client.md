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
| `wt new host NAME` | host | Prepare the guest, then run the default cloud-init recipe in Byobu |
| `wt ls` | retained | List kind, status, resources, disk use, and repository when present |
| `wt start NAME` | retained | Start the existing guest and disk |
| `wt stop NAME` | retained | Shut down the guest and keep its disk |
| `wt code NAME` | devcontainer | Open the live app workspace in VS Code |
| `wt ssh NAME` | retained | Sync managed aliases and connect to Byobu |
| `wt rm NAME` | retained | Destroy the world |
| `wt sync` | retained | Rewrite managed SSH inventory |

Each world has a disk file on the server. While it is running, `1.5G/32G disk`
means the file uses 1.5 GB of real disk space now and can grow to 32 GB. A
stopped world shows only `1.5G disk`. `wt stop` keeps the file; `wt rm` deletes
it.

New retained worlds require at least one valid regular `~/.ssh/*.pub` file.
Private keys are never sent to the server.

`scripts/install-client` creates a thin default host recipe at
`~/.config/wt/cloud-init.yaml` when the file is missing. It installs Diffo. The
retained image already contains Codex, and provisioning installs the `wt-codex-integration`
trampoline so shared sessions are indexed before Codex starts. The recipe
installs no Rust/Cargo toolchain or project checkout. The installer never
replaces an existing recipe.

WT does not resume failed world provisioning, including a partially installed
Codex trampoline. Remove the failed world with `wt rm`, then run `wt new` again;
the replacement starts from the retained image instead of repairing the disk.

Host setup does not receive the workstation SSH agent. Configured provider Git
operations use the gateway.

Every retained world receives the workstation's global Git `user.name` and
`user.email`; the client stops before contacting the server when either value is
missing. Devcontainer creation also requires an SSH-form Git source. `wt code`
requires the local `code` CLI and VS Code Remote-SSH extension.

## Managed SSH

When `~/.ssh/config` does not exist, `wt sync` creates it with:

```sshconfig
Include ~/.ssh/wt/config
```

WT never modifies an existing main configuration. Other global includes may
precede the WT include; it must only be outside any `Host` or `Match` block. WT
reports the manual change required when it is missing or scoped. `wt sync` owns
`~/.ssh/wt/config` and `~/.ssh/wt/known_hosts`. It pins host keys. Remote
contexts use their configured server as a jump host to the guest's private
address.

Devcontainer aliases are documented in
[Devcontainer worlds](../worlds/devcontainer.md#access). For hosts,
`CONTEXT.NAME` attaches to Byobu and `CONTEXT.NAME-vs` is direct guest SSH. WT
aliases do not enable SSH-agent forwarding. A developer can opt into native
OpenSSH forwarding for one direct connection with `ssh -A CONTEXT.NAME-vs`;
that unrestricted credential path bypasses gateway policy.

`wt new`, `wt ls`, `wt start`, `wt stop`, `wt rm`, and `wt ssh` synchronize automatically.
Run `wt sync` on another workstation after changing worlds elsewhere.
