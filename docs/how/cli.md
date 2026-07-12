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

## Create observation

After the daemon acknowledges a background create job, the client follows its
stored log. Ctrl-C stops observation, not provisioning. `wt logs` resumes from
stored output. If transport fails before acknowledgement, the client checks no
outcome automatically.

## SSH inventory

The main SSH config includes the managed file before every `Host` block:

```sshconfig
Include ~/.ssh/wt/config
```

`wt sync` converts running-world inventory into the aliases described in
[What WT does](../what/README.md#access). It owns `~/.ssh/wt/config` and
`~/.ssh/wt/known_hosts`; it does not edit the main SSH config.

Qualified aliases always exist. Short aliases exist only for globally unique
names. Host keys are pinned. Non-running worlds have no aliases.

Guest aliases for `bare_metal_ssh` contexts use the context's configured
OpenSSH host as a `ProxyJump`. OpenSSH connects to that server and asks it to
forward the connection to the guest's private libvirt address. Local contexts
connect to guest addresses directly. Direct app aliases retain their guest-side
proxy command, which composes with the guest's jump host.

Parent: [How WT works](./README.md).
