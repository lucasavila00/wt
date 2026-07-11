# Control plane

Parent: [architecture](./README.md). CLI: [cli.md](./cli.md).

```text
wt  ── local or OpenSSH stdio ──► wt-local api
                                      ├─ owner-scoped SQLite registry
                                      └─ wt-libvirt
                                             └─ libvirt/KVM worlds
```

| Piece | Role |
|-------|------|
| `wt` | Context-aware stdio client and SSH inventory manager |
| `wt-local` | Owner-scoped instance service and durable registry |
| `wt-libvirt` | KVM lifecycle, guest-agent provisioning, and inventory |

The helper reads one protocol version 3 JSON request from stdin and writes one
JSON response to stdout. Provisioning output uses stderr so stdout remains a
machine-readable protocol. There is no socket or HTTP listener.

The owner is the OS user executing `wt-local`. Local invocation uses the client
user; remote invocation uses the OpenSSH-authenticated site user. Each owner has
a registry at `~/.local/state/wt/instances-v2.db`.

SQLite stores the request, lifecycle state, final error, backend identifier, and
SSH inventory. Libvirt is the ground truth for VM existence. On restart, the
helper reopens the registry and inspects libvirt. If a recorded domain is gone,
the instance becomes `Error`; if DHCP changes an address, reconciliation updates
the endpoint while preserving the world's host-key identity.

Create returns success only after the guest is reachable over SSH, the requested
Git revision is checked out, and the stock devcontainer is running. Delete tears
down the domain and world files. The persisted running inventory is the source
for the client's managed OpenSSH files.
