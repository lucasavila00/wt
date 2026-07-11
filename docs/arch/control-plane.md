# Control plane and workers

Who owns inventory, how CLI finds instances, how we avoid zombies—without a heavy durable “platform DB.”  
Parent: [arch README](./README.md). Plan: [plan.md](../plan.md).

## Problem

- Mac / `~/.ssh/config` is **not** the control plane (CLI may only **print** Host snippets early).  
- Losing laptop state must not lose track of VMs.  
- Need “who has how many worlds?” and a path to kill **orphans**.  
- Do **not** care about billing or multi-tenant auth product.  
- Plan forward to **many workers** (bare-metal static, later k8s), GitLab-runner style—without a separate central process on day one.

## Roles

| Role | Job |
|------|-----|
| **Control plane** | CLI-facing API: create/list/destroy instances; fleet view; disposable aggregate state OK |
| **Worker** | Runs worlds (libvirt today, k8s later); **truth** for domains/pods on its machine; **reports** inventory upward |
| **CLI** | Points at **one** control-plane base URL only |

```text
CLI  ── control-plane API (wt-api) ──►  control plane
                                              ▲
                                              │ inventory / heartbeat / accept work
                                         worker(s)
                                              │
                                         VMs / pods
```

**Truth for “is this VM there?”** = worker + hypervisor/cluster.  
**Control plane** mirrors reports for fast list/query; can be **RAM-only** (or disposable Redis). Rebuild by workers re-reporting. No required long-lived business DB.

## Binaries (clarity over mode flags)

| Binary | Status | Role |
|--------|--------|------|
| **`wt`** | v1 | CLI |
| **`wt-local`** | v1 | **Control plane + embedded worker** on one host (single-site) |
| **`wt-control-plane`** | deferred | Control plane only; many workers report in |
| **`wt-worker`** | deferred | Worker only; `--control-plane-url …` |

Terms for roles: **control-plane** / **worker**. Never master/slave.  
User-facing story is **which binary you run**, not `--mode master`.

### v1: `wt-local`

```text
CLI ──► wt-local
          = control-plane HTTP API
          + embedded local worker (stub → libvirt)
```

- One process on the hypervisor.  
- CLI’s only URL is `wt-local`.  
- No Redis, no second daemon.  
- Implements the **same** control-plane API multi-node will use.

### Later: split processes

```text
CLI ──► wt-control-plane     (RAM/redis mirror + router)
              ▲
     report   │
         ┌────┴─────┐
         ▼          ▼
    wt-worker   wt-worker
    (libvirt)   (k8s, …)
```

- Same CLI, same control-plane API.  
- In-flight provision **stays on the worker**; control plane shows what workers report.  
- Prefer **shared libraries** (`control-plane` logic, `worker` logic) so `wt-local` is just both wired together—not a fork of the codebase.

## Zombies and GC

| Situation | Who fixes |
|-----------|-----------|
| Domain in libvirt, not in instance table | **Worker** reconcile; report orphan / GC |
| Instance record, domain gone | Worker marks error; control plane shows after report |
| Control plane empty after restart | Workers re-report (or `wt-local` rebuilds from libvirt) |
| Lost Mac ssh config | Irrelevant; re-print Host from CLI |

## What we explicitly do not build (yet)

- Durable central DB as product history  
- Billing, quotas-as-product, per-user IAM  
- CLI talking directly to workers  
- Shipping `wt-control-plane` / `wt-worker` binaries before multi-node hurts  

## Impl note

Era 1: **`wt-local`** with in-memory map + stub worker.  
Fleet report protocol + separate bins: when multi-host actually hurts.

## One-line summary

**CLI always talks to a control-plane API; v1 that is `wt-local` (plane + worker); later `wt-control-plane` + `wt-worker` without changing the CLI contract.**
