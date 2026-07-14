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

`wt new` returns after the guest and its SSH endpoint are ready. The first
`ssh NAME` forwards the workstation SSH agent and starts the remaining install
inside Byobu. Reconnecting attaches to the same session and retries a failed
installer with the newly forwarded agent socket.

## SSH inventory

The main SSH config includes the managed file before every `Host` block:

```sshconfig
Include ~/.ssh/wt/config
```

`wt sync` converts world inventory into the aliases described in
[What WT does](../what/README.md#access). It owns `~/.ssh/wt/config` and
`~/.ssh/wt/known_hosts`; it does not edit the main SSH config.

Qualified aliases always exist. Short aliases exist only for globally unique
names. Host keys are pinned. Setup worlds expose guest aliases with
`ForwardAgent yes`; running worlds additionally expose the app alias and no
longer forward the agent.

Guest aliases for `bare_metal_ssh` contexts use the context's configured
OpenSSH host as a `ProxyJump`. OpenSSH connects to that server and asks it to
forward the connection to the guest's private libvirt address. Local contexts
connect to guest addresses directly. Direct app aliases retain their guest-side
proxy command, which composes with the guest's jump host.

Parent: [How WT works](./README.md).
