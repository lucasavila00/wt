# Plan

Decided direction for build. Mental map, not a full design doc.  
Context (reasoning notes): [problem-statement.md](./plan-reasoning/problem-statement.md), [idealized-api.md](./plan-reasoning/idealized-api.md), [isolation-without-port-overrides.md](./plan-reasoning/isolation-without-port-overrides.md), [the-devcontainer-issue.md](./plan-reasoning/the-devcontainer-issue.md), [bare-metal-worlds.md](./plan-reasoning/bare-metal-worlds.md).  
**Architecture:** [arch/](./arch/README.md) (v1 = CLI + bare-metal agent; k8s deferred).

## Product

- **Named parallel instances** of an **existing** `.devcontainer` + Docker Compose recipe.
- Mac = cockpit only (CLI + `ssh`). No Docker on Mac.
- **`wt new <repo> <name>`** → world runs stock recipe → CLI writes **`~/.ssh/config`** → daily enter is **`ssh <name>`**.
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

## Providers (two agents, one CLI)

Like GitLab’s multiple executors: **one product API, two backends.**

| Provider | Role |
|----------|------|
| **Bare-metal agent** | Home / 1–2 fat servers: **KVM/libvirt guest per instance** |
| **k8s agent** | Company: **long-lived Pod world** with **Docker-in-Docker** (or equivalent) so stock compose runs *inside*; pod netns = no host port clash |

```text
Mac CLI + ssh config
        │
        ▼
   agent API (new / rm / ls …)
        │
   ┌────┴────┐
   ▼         ▼
 bare-metal  k8s
 (libvirt)   (DinD pod worlds)
```

- **Not reinventing k8s:** cluster schedules nodes; we own name→world, recipe, SSH Host, claim/GC.
- **KubeVirt:** optional later where available—**not** required worldwide.
- **Single home server via k8s/minikube just to run compose worlds = overkill.** Prefer libvirt there. Use k8s when multi-node / company ops already are k8s.

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

1. Shared **Rust** workspace + `wt-api` + CLI + **bare-metal agent** ([arch](./arch/README.md))  
2. End-to-end on one hypervisor: `new` → `ssh name` → stock compose  
3. Stabilize as single-dev daily driver  
4. **Only then** k8s agent ([arch/k8s-agent.md](./arch/k8s-agent.md))  
5. Polish lifecycle, multi-cluster selector, etc.

## One-line summary

**Stock devcontainer/compose in a per-instance world; CLI + SSH Host on the Mac; bare-metal libvirt at home and DinD-on-k8s at work—two providers, no new env format.**
