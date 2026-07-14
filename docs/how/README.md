# How WT works

```text
client: wt + OpenSSH
   │
   ├─ local ────────────────┐
   └─ ssh SERVER ───────────┤
                            ▼
                     wt-server api bridge
                            │
                     Unix socket (0600)
                            │
                     wt-server daemon
                       ├─ SQLite lifecycle registry
                       └─ wt-provider composite lifecycle
                          ├─ wt-libvirt
                          │  └─ KVM machine + QEMU guest transport
                          └─ world provisioner
                             ├─ /workspace Git checkout
                             ├─ Docker + devcontainer
                             └─ guest and app SSH
```

## Components

| Component | Owns |
|-----------|------|
| `wt` | Contexts, API transport, names, and managed SSH inventory |
| `wt-server` | Unix-socket API daemon, registry, durable jobs, and logs |
| `wt-provider` | Provider-neutral guest transport, embedded install flows, world provisioning, and composite lifecycle |
| `wt-libvirt` | KVM machine creation, inspection, destruction, and QEMU guest-agent transport |
| `wt-server-setup` | Embedded host setup, runtime config, golden image, and registry cache |
| `wt-guest` | Persistent app session and app SSH proxy helpers |

## Control plane

Local and remote contexts invoke `wt-server api`. The bridge carries one
versioned JSON request and response over stdio to the daemon's protected Unix
socket. There is no TCP control-plane listener. The installed server user owns
the daemon and registry records.

| Operation | Result |
|-----------|--------|
| `create` | Create and prepare a guest through SSH readiness |
| `list` | Return the owner's worlds and SSH inventory |
| `get` | Return one owned world |
| `delete` | Destroy one owned world |

Creation returns when the guest is ready for setup. The first app-shell SSH
connection forwards the workstation agent and runs the remaining installation
inside Byobu, so it and its guest-local log survive client disconnects. List,
get, and sync reconcile the completion marker into the running state.

## Data and trust

- Client contexts: `~/.wt/config.toml`.
- Managed SSH files: `~/.ssh/wt/config` and `~/.ssh/wt/known_hosts`.
- Runtime server config: `/etc/wt/server.toml`.
- User registry: `~/.local/state/wt/instances.db`.
- Checkout in each world: `/workspace`.
- Git private keys and passphrases never cross the API or enter server state.
- Client-to-server, server-to-Git, guest, and app SSH identities have distinct
  roles.

## Details

| Document | Contents |
|----------|----------|
| [Client and SSH](./cli.md) | Context transport and SSH inventory generation |
| [Libvirt/KVM](./bare-metal-agent.md) | World lifecycle |
| [Provider architecture](./provider-api.md) | Machine providers, guest transport, and world provisioning |
| [Registry cache](./registry-cache.md) | Shared image-blob cache |

User-visible behavior: [What WT does](../what/README.md).
