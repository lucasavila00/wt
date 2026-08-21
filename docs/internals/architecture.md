# Architecture

```text
wt client
  └─ local process or OpenSSH ── wt-server API bridge
                                  └─ Unix socket (0600)
                                      └─ wt-server
                                          ├─ shared registry and capacity
                                          ├─ devcontainer lifecycle
                                          └─ host lifecycle

wt-gh-actions-runner library foundation
  └─ GitHub CI lifecycle ─────── shared registry and capacity

devcontainer / host / github-ci
  └─ wt-libvirt-kvm ── KVM + QEMU guest agent

standalone Git client
  └─ OpenSSH forced command ── wt-git-proxy ── SSH Git upstream
```

`wt-server` owns retained worlds. The CI library uses the same registry
admission model; its operator process is not shipped yet.

The control plane has no TCP listener. Local and remote API bridges send one
versioned JSON request over stdio to the protected server socket. Protocol
Protocol version 4 carries tagged world kinds, a common retained-world Git
author, and context-local Codex session observations.

## Crates

| Scope | Crates |
|-------|--------|
| WT | `wt-client`, `wt-control-protocol`, `wt-server`, `wt-retained-worlds`, `wt-devcontainer-guest-tools`, `wt-codex-integration`, `wt-server-installer` |
| GitHub Actions | `wt-gh-actions-runner` |
| Agent tool gateway | `wt-agent-tool-gateway`, `wt-tools` |
| Standalone Git proxy | `wt-git-proxy`, `wt-git-proxy-installer` |
| Shared | `wt-libvirt-kvm`, `wt-workload-registry`, `wt-git-smart-protocol`, `wt-installer-support` |
| Tests | `wt-end-to-end-tests` |

Generic names are used only for behavior shared by more than one kind.
Installed executable names match their owning crates.

`wt-git-smart-protocol` contains the Git protocol bridge and branch write policy shared
by the WT gateway and standalone proxy. `wt-git-proxy` is released from this
workspace but is not part of `wt-server` or a WT world.

`wt-installer-support` contains the host file, command runner, and SSH credential
handling shared by the regular WT and standalone Git proxy installers.

`wt-retained-worlds` owns devcontainer and host lifecycle, the fixed retained
guest identity, and operations shared by both retained kinds. Its runtime calls the image-installed
helpers for SSH access, Git author transfer, agent tooling, and virtiofs
Codex session and authentication mounts. One retained provisioning operation
applies that complete contract for both kinds; kind crates retain only their
application-specific setup.

Retained-world provisioning is intentionally restart-only. WT does not resume
an interrupted sequence or repair its partial guest state. Remove a failed
world and create it again from the retained image; a healthy provisioning run
normally takes about 5–10 seconds.

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
