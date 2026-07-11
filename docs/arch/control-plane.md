# Control plane and workers

Parent: [arch README](./README.md). CLI: [cli.md](./cli.md).

## Era 1

```text
wt  →  local wt-local helper
             ├─ SQLite instance registry
             └─ wt-libvirt
                    └─ libvirt/KVM worlds
```

| Piece | Role |
|-------|------|
| `wt` | Local stdio client |
| `wt-local` | Owner-scoped instance service + durable registry |
| `wt-libvirt` | KVM domain lifecycle, guest agent, inventory |

Owner = local OS user. No public listener. No SSH transport.

Ground truth for VM existence = libvirt. SQLite holds requested instance state and backend id.

## Reconcile

| Situation | Handling |
|-----------|----------|
| Domain exists, no record | Later GC/reconcile policy |
| Record exists, domain missing | Mark error |
| Helper restarted | Reopen SQLite; inspect libvirt |

## Era 1.5

Create adds Git source and optional ref. The worker returns success only after guest SSH, checkout, and Compose are ready. SQLite persists the request, final error, and the instance's SSH user, current address, port, and public host keys. Reconciliation refreshes a changed DHCP address from libvirt while the host keys remain the world's stable SSH identity. This inventory is the source for `wt sync`; it does not grant `wt-local` an SSH transport or expose the guest filesystem locally.

## Era 2

```text
client wt  →  ssh site -- wt-local api  →  same registry + worker
```

Site SSH changes control transport only. Guest SSH already exists from Era 1.5; the API and ownership model stay the same.

## Later

- Multi-node `wt-control-plane` + `wt-worker`
- Kubernetes worker

## One-line summary

**`wt-local` owns instance state; `wt-libvirt` owns KVM worlds.**
