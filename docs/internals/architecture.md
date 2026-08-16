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
```

`wt-server` owns retained worlds. The CI library uses the same registry
admission model; its operator process is not shipped yet.

The control plane has no TCP listener. Local and remote API bridges send one
versioned JSON request over stdio to the protected server socket. Protocol
version 1 carries tagged world kinds.

## Crates

| Scope | Crates |
|-------|--------|
| Shared | `wt-api`, `wt-cli`, `wt-command`, `wt-provider`, `wt-libvirt`, `wt-registry`, `wt-server`, `wt-server-setup`, `wt-agent-git`, `wt-integration-tests` |
| Devcontainer | `wt-devcontainer`, `wt-devcontainer-guest` |
| Host | `wt-host` |
| GitHub CI | `wt-github-ci` |

Generic names are used only for behavior shared by more than one kind.
Executable names used inside existing guests remain stable.

## State

- Client contexts: `~/.wt/config.toml`
- Managed SSH: `~/.ssh/wt/`
- Server configuration: `/etc/wt/server.toml`
- Capacity configuration: `/etc/wt/capacity.toml`
- Shared registry: `~/.local/state/wt/instances.db`
- KVM machine files: the configured libvirt worlds directories

Application data is typed by kind. A host record cannot acquire devcontainer
checkout or app-SSH state, and a CI record cannot appear in retained-world
inventory.
