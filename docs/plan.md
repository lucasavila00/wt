# Plan

Product direction. Architecture: [arch/](./arch/README.md). Implementation order: [impl/](./impl/README.md).  
Background notes: [plan-reasoning/](./plan-reasoning/).

## Product

- **Named parallel instances** of an existing `.devcontainer` + Docker Compose recipe.
- **Mac = cockpit only** (CLI + `ssh`). No Docker on the Mac.
- **`wt new <repo> <name>`** → site runs stock recipe → CLI **prints** an SSH `Host` snippet → daily enter is **`ssh <name>`** (after the user applies the snippet; automatic ssh-config edit is a later UX polish item, not required for the core loop).
- **Isolation** = each instance has its own network identity so stock `"3000:3000"` works N times. **Trusted pool** (solo or same company)—not hostile multi-tenant security.

## Recipe

- Canonical recipe = the team’s existing **`.devcontainer` + Compose**. No parallel env format for `wt`.
- GitLab CI stays a separate batch path; shared contract with interactive dev is mainly **images** (+ discipline), not one mega-file.

## World

```text
world = small Linux with Docker + own netns/IP
        clone repo → stock compose/devcontainer up
```

## Control plane and workers

Detail: [arch/control-plane.md](./arch/control-plane.md).

| Piece | Role |
|-------|------|
| **CLI (`wt`)** | One control-plane base URL only |
| **Control plane** | Create/list/destroy instances; fleet view; state may be disposable RAM |
| **Worker** | Runs worlds; ground truth on that host/cluster; inventory for the control plane |

**Current deploy shape:** binary **`wt-local`** = control plane + embedded bare-metal worker on one hypervisor. CLI points at `wt-local`.

**Multi-node shape (not built yet):** **`wt-control-plane`** + **`wt-worker`** (libvirt and/or k8s). Same CLI API.

## Worker backends

| Backend | Use |
|---------|-----|
| **Bare-metal (libvirt KVM)** | Home / fat servers; one guest per instance |
| **k8s (DinD pod worlds)** | Company dev clusters that allow it; not built yet |

Compose authors never target “our platform.” Multiplicity is outside the app repo.

## Bare metal

- KVM guests on the big box ([plan-reasoning/bare-metal-worlds.md](./plan-reasoning/bare-metal-worlds.md)).
- Typical instance size **≥16 GB** → guest OS overhead is minor; empty-guest boot is minor vs clone + compose.

## Out of scope

| Out of scope | Why |
|--------------|-----|
| New recipe language replacing compose/devcontainer | Existing recipe is the contract |
| Security tenancy / sandboxing coworkers | Trusted pool; care about ports, not neighbors-as-attackers |
| k8s on a single personal server as the home path | `wt-local` + libvirt is enough |
| KubeVirt required on every company cluster | Optional where present; DinD-friendly pool is the k8s path |
| CI system of record or git-worktree manager | Interactive world multiplicity only |

## Build order

1. `wt-api` + `wt` + `wt-local` ([arch](./arch/README.md), [impl](./impl/README.md))  
2. Libvirt guest + real SSH  
3. Stock recipe in guest  
4. Daily-driver UX (including optional ssh-config apply)  
5. Library seams for multi-node bins; then k8s worker when needed  

## One-line summary

**Stock devcontainer/compose per named world; `wt` on the Mac; `wt-local` on the site; later optional `wt-control-plane` + `wt-worker`.**
