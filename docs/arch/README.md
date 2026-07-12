# Architecture

```text
client: wt + OpenSSH
   │
   ├─ local ────────────────┐
   └─ ssh SERVER ───────────┤
                            ▼
                     wt-server api
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
| `wt-server` | Owner-scoped API, registry, durable jobs, and logs |
| `wt-libvirt` | KVM world creation, inspection, and destruction |
| `wt-server-setup` | Host setup, runtime config, golden image, and registry cache |
| `wt-guest` | Persistent app session and app SSH proxy helpers |

## Control plane

Local and remote contexts invoke `wt-server api`. The transport carries one
versioned JSON request and response over stdio. There is no control-plane socket.
The OS user running `wt-server` owns the request and registry records.

| Operation | Result |
|-----------|--------|
| `create` | Reserve and start detached provisioning |
| `list` | Return the owner's worlds and SSH inventory |
| `get` | Return one owned world |
| `delete` | Destroy one owned world |
| `logs` | Read provisioning output from a byte offset |

Provisioning is acknowledged after SQLite records the job and the detached
worker accepts it. Client disconnects after acknowledgement do not stop the job.
A worker crash changes the world to `error` on the next API operation; partial
resources remain until `wt rm`.

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
| [CLI and SSH](./cli.md) | Contexts, naming, commands, and aliases |
| [Libvirt/KVM](./bare-metal-agent.md) | World lifecycle |
| [Registry cache](./registry-cache.md) | Shared image-blob cache |

Product scope: [product.md](../product.md).
