# Plan

Product direction. Architecture: [arch/](./arch/README.md). Implementation order: [impl/](./impl/README.md).  
Background notes: [plan-reasoning/](./plan-reasoning/).

## Product

- **Named parallel instances** of an existing `.devcontainer` + Docker Compose recipe.
- **Mac = cockpit only** (CLI + `ssh`). No Docker on the Mac.
- **`wt new <source> <name>`** with **`name` = `{repo}-{feature}`** → site runs stock recipe → CLI **prints** guest Host snippet; **`wt sync`** projects **my** worlds into managed ssh config; enter with **`ssh <name>`** / **`wt ssh`**.  
- **Clusters = contexts (sum type):** **`bare_metal_ssh`** (CLI → `ssh user@host -- helper`) and **`bare_metal_local`** (CLI → helper on this machine, no SSH)—same `wt-local` JSON API. Later **`k8s`** is another variant. Owner = SSH user or local OS user. Detail: [arch/cli.md](./arch/cli.md).
- **Isolation** = each instance has its own network identity so stock `"3000:3000"` works N times. **Trusted pool** (solo or same company)—not hostile multi-tenant security.

## Recipe

- Canonical recipe = the team’s existing **`.devcontainer` + Compose**. No parallel env format for `wt`.
- GitLab CI stays a separate batch path; shared contract with interactive dev is mainly **images** (+ discipline), not one mega-file.
- Era 1.5 may use the invoking site user's existing `ssh-agent` for private SSH clones. This is a provisional convenience assumption, not the final Git credential model or a blocker for validating the world lifecycle.

## World

```text
world = small Linux with Docker + own netns/IP
        clone repo → stock compose/devcontainer up
```

## Control plane and workers

Detail: [arch/control-plane.md](./arch/control-plane.md).

| Piece | Role |
|-------|------|
| **CLI (`wt`)** | Context → spawn helper (SSH or local); logical instance API over stdio JSON |
| **Control plane** | Create/list/destroy (owner-scoped); invoked as helper command |
| **Worker** | Runs worlds; ground truth; inventory for the control plane |

**Deploy order:**
- Era 1/1.5: **`wt` + `wt-local`** on the same Ubuntu workstation.
- Era 2: client **`wt`** invokes remote **`wt-local`** through OpenSSH.
No public control-plane HTTP.

**Multi-node shape (not built yet):** **`wt-control-plane`** + **`wt-worker`**. Prefer SSH (or equivalent private path) to the plane.

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

1. Local bare-metal vertical slice: Docker/Compose-ready KVM guest
2. Era 1.5: Git clone + ref checkout + Compose up inside the local world, then SSH/VS Code access to that usable environment
3. Era 2: remote client → site helper through OpenSSH
4. Multi-node bins and k8s worker when needed

## One-line summary

**Stock devcontainer/compose per named world; `wt` on the Mac; `wt-local` on the site; later optional `wt-control-plane` + `wt-worker`.**
