# Architecture

```text
wt client
  └─ local process or OpenSSH ── wt-server API bridge
                                  └─ Unix socket (0600)
                                      └─ wt-server
                                          ├─ registry and capacity
                                          └─ host lifecycle

host world ── wt-libvirt-kvm ── KVM + QEMU guest agent

standalone Git client
  └─ OpenSSH forced command ── wt-git-proxy ── SSH Git upstream
```

`wt-server` owns host worlds. The control plane has no TCP listener. Local
and remote API bridges send one versioned JSON request over stdio to the
protected server socket. The protocol carries world resources, a Git author,
server information, context-local Codex session observations, and streamed
creation progress events.

## Crates

| Scope | Crates |
|-------|--------|
| WT | `wt-client`, `wt-control-protocol`, `wt-server`, `wt-host-world`, `wt-codex-integration`, `wt-server-installer` |
| Agent tool gateway | `wt-agent-tool-gateway`, `wt-tools` |
| Standalone Git proxy | `wt-git-proxy`, `wt-git-proxy-installer` |
| Shared | `wt-libvirt-kvm`, `wt-workload-registry`, `wt-git-smart-protocol`, `wt-installer-support` |
| Tests | `wt-end-to-end-tests` |

Installed executable names match their owning crates.

`wt-git-smart-protocol` contains the Git protocol bridge and branch write
policy shared by the WT gateway and standalone proxy. `wt-git-proxy` is
released from this workspace but is not part of `wt-server` or a WT world.

`wt-installer-support` contains host file, command runner, and SSH credential
handling shared by the regular WT and standalone Git proxy installers.

`wt-host-world` owns host lifecycle and the fixed guest identity. Its
runtime calls image-installed helpers for SSH access, Git author transfer,
agent tooling, and virtiofs Codex session and authentication mounts.

Codex lifecycle hooks report session identity, pane order, and activity only.
The guest relay independently polls registered working directories for Git
context and sends authenticated metadata updates to the host. Those updates
cannot create, reorder, reactivate, or refresh a lifecycle observation.

The installer builds one development-tools image. It owns current language
toolchains, build and CLI tools, and Docker/Compose with recorded resolved
versions. This is not a runtime world setting.

Provisioning is intentionally restart-only. WT does not resume an interrupted
sequence or repair partial guest state. Remove a failed world and create it
again from the host image.

## State

- Client contexts: `~/.wt/config.toml`
- Managed SSH: `~/.ssh/wt/`
- Server configuration: `/etc/wt/server.toml`
- Capacity configuration: `/etc/wt/capacity.toml`
- Registry: `~/.local/state/wt/instances.db`
- KVM machine files: the configured libvirt worlds directories

Each registry record is a host world with its resources, backend,
disk, SSH endpoint, and gateway grant.
