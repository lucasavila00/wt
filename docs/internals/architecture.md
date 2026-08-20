# Architecture

```text
wt client
  └─ local process or OpenSSH ── wt-server API bridge
                                  └─ Unix socket (0600)
                                      └─ wt-server
                                          ├─ shared registry and capacity
                                          ├─ devcontainer lifecycle
                                          └─ host lifecycle

wt-github-ci library foundation
  └─ GitHub CI lifecycle ─────── shared registry and capacity

devcontainer / host / github-ci
  └─ wt-provider contracts ──── wt-libvirt ── KVM + QEMU guest agent

standalone Git client
  └─ OpenSSH forced command ── wt-git-proxy ── SSH Git upstream
```

`wt-server` owns retained worlds. The CI library uses the same registry
admission model; its operator process is not shipped yet.

The control plane has no TCP listener. Local and remote API bridges send one
versioned JSON request over stdio to the protected server socket. Protocol
version 1 carries tagged world kinds.

## Crates

| Scope | Crates |
|-------|--------|
| Shared | `wt-api`, `wt-cli`, `wt-command`, `wt-provider`, `wt-libvirt`, `wt-registry`, `wt-server`, `wt-server-setup`, `wt-setup-core`, `wt-git-core`, `wt-integration-tests` |
| Devcontainer | `wt-devcontainer`, `wt-devcontainer-guest` |
| Host | `wt-host` |
| GitHub CI | `wt-github-ci` |
| WT Git gateway | `wt-agent-git` |
| Standalone Git proxy | `wt-git-proxy`, `wt-git-proxy-setup` |

Generic names are used only for behavior shared by more than one kind.
Executable names used inside existing guests remain stable.

`wt-git-core` contains the Git protocol bridge and branch write policy shared
by the WT gateway and standalone proxy. `wt-git-proxy` is released from this
workspace but is not part of `wt-server` or a WT world.

`wt-setup-core` contains the host file, command runner, and SSH credential
handling shared by the regular WT and standalone Git proxy installers.

## State

- Client contexts: `~/.wt/config.toml`
- Default host cloud-init: `~/.config/wt/cloud-init.yaml`
- Managed SSH: `~/.ssh/wt/`
- Server configuration: `/etc/wt/server.toml`
- Capacity configuration: `/etc/wt/capacity.toml`
- Shared registry: `~/.local/state/wt/instances.db`
- KVM machine files: the configured libvirt worlds directories

Application data is typed by kind. A host record cannot acquire devcontainer
checkout or app-SSH state, and a CI record cannot appear in retained-world
inventory.
