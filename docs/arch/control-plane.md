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

Create adds Git source and optional ref. The worker returns success only after Compose is ready. SQLite persists the request and final error.

## Era 2

```text
client wt  →  ssh site -- wt-local api  →  same registry + worker
```

SSH changes transport only. The API and ownership model stay the same.

## Later

- Multi-node `wt-control-plane` + `wt-worker`
- Kubernetes worker

## One-line summary

**`wt-local` owns instance state; `wt-libvirt` owns KVM worlds.**
