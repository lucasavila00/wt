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
                       ├─ SQLite registry and logs
                       └─ wt-libvirt
                            └─ KVM world
                                ├─ /workspace Git checkout
                                ├─ Docker + devcontainer
                                └─ guest and app SSH
```

## Components

| Component | Owns |
|-----------|------|
| `wt` | Contexts, API transport, names, and managed SSH inventory |
| `wt-server` | Unix-socket API daemon, registry, durable jobs, and logs |
| `wt-libvirt` | KVM world creation, inspection, and destruction |
| `wt-server-setup` | Host setup, runtime config, golden image, and registry cache |
| `wt-guest` | Persistent app session and app SSH proxy helpers |

## Control plane

Local and remote contexts invoke `wt-server api`. The bridge carries one
versioned JSON request and response over stdio to the daemon's protected Unix
socket. There is no TCP control-plane listener. The installed server user owns
the daemon and registry records.

| Operation | Result |
|-----------|--------|
| `create` | Reserve and start background provisioning |
| `list` | Return the owner's worlds and SSH inventory |
| `get` | Return one owned world |
| `delete` | Destroy one owned world |
| `logs` | Read provisioning output from a byte offset |

Provisioning is acknowledged after SQLite records the job and the daemon starts
its background worker. Client disconnects after acknowledgement do not stop the
job. A daemon crash or restart changes interrupted worlds to `error` during
startup; partial resources remain until `wt rm`.

## Data and trust

- Client contexts: `~/.wt/config.toml`.
- Managed SSH files: `~/.ssh/wt/config` and `~/.ssh/wt/known_hosts`.
- Runtime server config: `/etc/wt/server.toml`.
- User registry: `~/.local/state/wt/instances.db`.
- Checkout in each world: `/workspace`.
- Git passphrases cross the API for provisioning and are never persisted.
- Client-to-server, server-to-Git, guest, and app SSH identities have distinct
  roles.

## Details

| Document | Contents |
|----------|----------|
| [Client and SSH](./cli.md) | Context transport and SSH inventory generation |
| [Libvirt/KVM](./bare-metal-agent.md) | World lifecycle |
| [Registry cache](./registry-cache.md) | Shared image-blob cache |

User-visible behavior: [What WT does](../what/README.md).
