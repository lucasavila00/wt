# Control plane and workers

Parent: [arch README](./README.md). Plan: [plan.md](../plan.md).

## Roles

| Role | Job |
|------|-----|
| **Control plane** | CLI-facing API: create/list/destroy; fleet view. State may be RAM-only / disposable. |
| **Worker** | Runs worlds; ground truth for VMs/pods on its machine or cluster; supplies inventory to the control plane. |
| **CLI** | One control-plane base URL. Does not address workers directly. |

```text
CLI  ── wt-api (HTTP/JSON) ──►  control plane
                                      ▲
                                      │ inventory / heartbeat / work
                                 worker(s)
                                      │
                                 VMs / pods
```

Ground truth for “does this VM exist?” = **worker + hypervisor/cluster**.  
Control plane is a **query and orchestration surface**; it need not be a durable business database. After a control-plane restart, workers re-report (or `wt-local` rebuilds from libvirt on the box).

Mac ssh config is **not** inventory.

## Binaries

| Binary | Status | Role |
|--------|--------|------|
| **`wt`** | in workspace | CLI |
| **`wt-local`** | in workspace | Control plane **and** embedded worker on one host |
| **`wt-control-plane`** | not built | Control plane only; multiple workers report in |
| **`wt-worker`** | not built | Worker only (libvirt or k8s) |

```text
# single site (what we run now)
CLI ──► wt-local

# multi-node (target shape)
CLI ──► wt-control-plane
              ▲
         wt-worker …
```

Shared libraries should hold control-plane and worker logic so `wt-local` is a composition of both; multi-node bins reuse the same logic.

## Zombies and GC

| Situation | Handling |
|-----------|----------|
| Domain in libvirt, no instance record | Worker reconcile → orphan GC or surface on list |
| Instance record, domain missing | Worker marks error; visible via control plane |
| Control plane process restarted empty | Rebuild from worker reports / local libvirt |
| Lost laptop ssh config | Re-print Host from CLI; VMs unchanged |

## Out of scope for this layer

- Durable central DB for product history  
- Billing / per-user IAM product  
- CLI → worker direct protocol  

## One-line summary

**CLI always hits the control-plane API; on a single site that is `wt-local`.**
