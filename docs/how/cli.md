# Client transport and SSH inventory

## Context transport

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

- A local context runs `wt-server api`.
- An OpenSSH context runs `ssh -- HOST wt-server api`.
- Requests and responses use the same JSON protocol over stdio.
- Multi-context operations fail if any context fails.

The client resolves `context.world` directly. It resolves a short name only when
the name is unique across all contexts.

## World setup

`wt new` requires a terminal and guides the user through interactive prompts.
World name, Git repository, and base branch are required. Context, CPU, RAM,
disk, and confirmation have defaults. The client validates every answer and
reads all regular `~/.ssh/*.pub` files before it sends one create request.

The request owns the world resources and authorized keys. The server config
owns only infrastructure, image-build settings, trust, and timeouts.

`wt ls` shows each world's context, name, status, repository name, requested
CPU/RAM/disk resources, and any lifecycle error. Guest IP addresses
and raw SSH endpoints are omitted because managed world aliases are the client
connection interface.

The server gives the world a scoped gateway grant, clones through the gateway,
and finishes the devcontainer setup. After the guest SSH endpoint is ready,
`wt new` synchronizes the SSH inventory and replaces itself with
`ssh CONTEXT.NAME`. Reconnect with `ssh NAME` to attach to the same Byobu
session.

## Git inside a world

Every checkout uses an `ag::` origin. Normal `git fetch`, `git pull`, and
`git push` go through the local guest relay; the world has no provider key or
token. A world named `df1` can push branches under `df1/`:

```bash
git switch -c df1/fix-login
git push
```

The gateway rejects other branch namespaces and tags. Run `ag-git` for the
current branch's pull or merge request, reviews, and CI, or `ag-git --help` for
the complete command list.

## VS Code launch

`wt code NAME` requires a complete world inventory, resolves `NAME` using the
normal qualified-or-globally-unique rules, and updates the managed SSH files.
For a running world, it asks the guest's `wt-app-info` helper for the primary
devcontainer's live workspace destination and launches:

```text
code --remote ssh-remote+CONTEXT.WORLD-vs WORKSPACE
```

The workspace destination comes from the container mount rather than assuming a
fixed `/workspaces/...` path.

## SSH inventory

The main SSH config includes the managed file before every `Host` block:

```sshconfig
Include ~/.ssh/wt/config
```

`wt sync` converts world inventory into the aliases described in
[What WT does](../what/README.md#access). It owns `~/.ssh/wt/config` and
`~/.ssh/wt/known_hosts`; it does not edit the main SSH config.

Qualified aliases always exist. Short aliases exist only for globally unique
names. Host keys are pinned. Generated aliases do not forward the workstation
SSH agent. A developer may explicitly use `ssh -A NAME-host` for the guest or
`ssh -A NAME-vs` for the devcontainer; WT does not relay that agent through the
`NAME` Byobu path. Running worlds add the app alias.

Guest aliases for `bare_metal_ssh` contexts use the context's configured
OpenSSH host as a `ProxyJump`. OpenSSH connects to that server and asks it to
forward the connection to the guest's private libvirt address. Local contexts
connect to guest addresses directly. Direct app aliases retain their guest-side
proxy command, which composes with the guest's jump host.

Parent: [How WT works](./README.md).
