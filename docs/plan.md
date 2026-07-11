# Plan

Decided direction for build. Mental map, not a full design doc.  
Context (reasoning notes): [problem-statement.md](./plan-reasoning/problem-statement.md), [idealized-api.md](./plan-reasoning/idealized-api.md), [isolation-without-port-overrides.md](./plan-reasoning/isolation-without-port-overrides.md), [the-devcontainer-issue.md](./plan-reasoning/the-devcontainer-issue.md), [bare-metal-worlds.md](./plan-reasoning/bare-metal-worlds.md).  
**Architecture:** [arch/](./arch/README.md) (v1 = `wt` + `wt-local`; k8s / multi-node bins deferred).  
**Implementation eras:** [impl/](./impl/README.md).

## Product

- **Named parallel instances** of an **existing** `.devcontainer` + Docker Compose recipe.
- Mac = cockpit only (CLI + `ssh`). No Docker on Mac.
- **`wt new <repo> <name>`** → world runs stock recipe → CLI **prints** (later may apply) SSH Host → daily enter is **`ssh <name>`**.
- Isolation = **port/network multiplicity** (stock `"3000:3000"` N times), not hostile multi-tenant security. **Trusted pool** (solo or same company).

## Recipe (canonical)

- **Same devcontainer/compose the team already uses.** No new env format; no “wt YAML” that replaces compose.
- GitLab CI stays a **separate batch** wiring; parity is **same images** (and discipline), not one mega-file.
- Do **not** invent a GitLab-CI-like format just to map to k8s—port isolation is a **runtime** property of worlds, not a reason to drop compose.

## World invariant

```text
world = small Linux with Docker + own netns/IP
        clone repo → stock compose/devcontainer up
```

Compose authors never target “our platform.” Multiplicity is outside the app repo.

## Control plane + workers (GitLab-shaped, simple)

Detail: [arch/control-plane.md](./arch/control-plane.md).

- **CLI always talks to one control-plane URL** (never to “random hypervisors” as the product path).
- **Worker** = runs worlds (libvirt / later k8s); **truth** for what VMs/pods exist; **reports** inventory upward.
- **Control plane** = list/create/destroy API + fleet view; state may be **RAM-only / disposable cache**—rebuild from worker reports. No required central business DB. Not for billing/auth product.
- Mac ssh config is **not** inventory (print Host early; auto-edit later).

**v1 (fewest moving parts):** binary **`wt-local`** = control plane + embedded local worker. CLI points at it—it *is* the site control plane.

**Later (separate binaries, not mode flags):** **`wt-control-plane`** (aggregate) + **`wt-worker`** (report to remote URL)—static bare-metal and k8s executors, GitLab-runner style. Role names: **control-plane** / **worker** only—not master/slave.

## Providers (worker backends, one CLI API)

Like GitLab’s multiple executors: **one control-plane API, two worker backends.**

| Worker backend | Role |
|----------------|------|
| **Bare-metal** | Home / 1–2 fat servers: **KVM/libvirt guest per instance** |
| **k8s** | Company: **long-lived Pod world** with **Docker-in-Docker** (or equivalent); pod netns = no host port clash |

```text
Mac CLI (`wt`)
   │  control-plane API only
   ▼
v1: wt-local                 later: wt-control-plane
   (plane + embed worker)              ▲ report
                                       │
                              wt-worker (libvirt | k8s)
```

- **Not reinventing k8s:** cluster schedules pods; we own name→world, recipe, SSH Host print/apply, claim/GC via workers.
- **KubeVirt:** optional later where available—**not** required worldwide.
- **Single home server via k8s/minikube just to run compose worlds = overkill.** Prefer libvirt worker there.

### Company requirement

A **dev/workspaces cluster (or node pool)** that **allows DinD-class worlds** (often privileged). Locked-down prod clusters that forbid this are **out of scope** for the k8s provider unless another world engine exists (e.g. KubeVirt).

### Horizontal scale

- Bare metal: more hypervisors / pool size (agent inventory).
- k8s: more nodes / multi-cluster via kubecontext—same `wt` / `ssh` UX.

## Bare metal lean

- Prefer **KVM** over LXD for stable stock Docker DX ([bare-metal-worlds.md](./plan-reasoning/bare-metal-worlds.md)).
- Assume **≥16 GB/instance** → VM OS tax is noise; empty-world boot ≪ clone + compose up.

## Explicitly out of scope (we do not care)

These are **decisions**, not backlog. We are not building toward them later “if we have time.”

| We do not care about | Because |
|----------------------|---------|
| A **new recipe language** replacing compose/devcontainer | Canonical recipe is the one the team already has |
| **Security tenancy** / sandboxing coworkers | Trusted pool; isolation means ports, not neighbors-as-attackers |
| **k8s on a single personal server** as the home path | Overkill; libvirt is enough when compose is the world |
| **KubeVirt required** on every company cluster | Optional where it exists; DinD-friendly dev cluster is the k8s path |
| Being **CI system of record** or a **git-worktree manager** | Multiplicity of interactive worlds only; CI and git stay separate tools |

## Build order (intent)

1. Shared **Rust** workspace + `wt-api` + `wt` + **`wt-local`** ([arch](./arch/README.md))  
2. End-to-end on one hypervisor: `new` → `ssh name` → stock compose  
3. Stabilize as single-dev daily driver  
4. **Only then** multi-node bins / k8s worker ([control-plane](./arch/control-plane.md), [k8s](./arch/k8s-agent.md))  
5. Polish lifecycle, multi-cluster selector, etc.

## One-line summary

**Stock devcontainer/compose in a per-instance world; `wt` on the Mac; v1 server is `wt-local`; later `wt-control-plane` + `wt-worker`—no new env format.**
