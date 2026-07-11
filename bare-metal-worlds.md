# Bare-metal worlds (one big server)

How to run **N instances** on **one** fat host without port soup. Mental map from discussion—not full ops runbook.  
Context: [isolation-without-port-overrides.md](./isolation-without-port-overrides.md), [idealized-api.md](./idealized-api.md). Trust model: **trusted pool** (solo or same company), not hostile multi-tenant.

## Constraint reminder

- Need **per-instance network identity** (own IP / publish space) so stock `"3000:3000"` works N times.
- Do **not** need security sandboxing of neighbors.
- App/compose RAM dominates and is **not decidable** up front. Planning assumption: **≥16 GB per instance** is normal.
- Create path always pays **clone + images + compose/devcontainer up**. Hypervisor/container **boot** of an empty world is not the long pole.

## Options

### 1. One Docker, many IPs (macvlan / host addresses)

One kernel, one Docker. Each instance publishes on **its** IP (`10.0.0.11:3000` vs `10.0.0.12:3000`).

| + | − |
|---|---|
| Max density | One daemon; everything visible together |
| Fast “create” (no guest) | `ssh <name>` ≠ natural mini-host unless extra work |
| | Publish often needs tool-owned bind-IP (not always 100% stock compose verbatim) |

### 2. LXD / Incus system containers

Each instance = system container, own netns/IP; **Docker runs inside**. Compose authors see a normal Linux.

| + | − |
|---|---|
| Dense; no guest kernel | Nested Docker: privileges, storage, edge-case footguns |
| `ssh <name>` maps cleanly if unit has the IP | Complexity is **yours** (golden profile), not app authors—if done right |
| Snapshots/clones | “Works in VM, fails in LXD” until profile is boring |

**DX:** Can be transparent: authors never hear LXD; only SSH + stock compose **inside** the unit.

### 3. KVM / libvirt guests

Each instance = VM, own IP, own Docker. Closest to “mini server from the pool.”

| + | − |
|---|---|
| Easiest path to **stable + transparent** stock Docker/compose | Heavier idle tax (~0.5–1 GB OS); worse density |
| Dumb failure modes; no Docker-in-container games | Image + libvirt/cloud-init ops |
| `ssh <name>` → guest IP is obvious | |

## RAM: does KVM matter?

- Idle KVM floor often **~0.5–1 GB** (budget **~1 GB** for empty-ish warm guest).
- LXD saves mostly that per-instance OS/kernel tax.
- At **≥16 GB/instance**, KVM overhead is **noise** (~few percent). Compose/app size dominates either way.
- So RAM is **not** a strong reason to pick 2 over 3 under this sizing.

## Boot time: does LXD matter?

Faster guest/container start for 2 vs 3 is real but **usually irrelevant** for this product:

```text
claim world → (optional short boot) → clone → pull/build → compose/devcontainer up
                 ^^^^^^^^^^^^^^^^      ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
                 small                dominates wall clock
```

You pay recipe spin-up on every new instance regardless of hypervisor. Optimize **golden image + warm pool + image cache**, not micro-VM boot.

## Lean (non-binding)

| Priority | Choice |
|----------|--------|
| Stable, transparent DX first | **3 KVM** on the big box |
| Density later, own nesting | **2 LXD** with Docker-in-unit, same SSH story |
| Max pack, accept weaker world boundary | **1** multi-IP Docker |

**Default bias:** **KVM guests on bare metal** as the world factory. One huge server = hypervisor + pool of golden VMs (or cold-clone from template). Agent claims a free guest, runs recipe, CLI writes `Host <name>`.

**Transparency rule:** world = “small Linux with Docker.” Never “special host Docker that compose authors must target.”

## Open

1. Warm pool size vs cold clone-from-template.  
2. How IPs are assigned (bridge + DHCP vs static).  
3. Whether LXD is a later density migration or never.

## One-line summary

On one fat trusted host, prefer **KVM-per-instance** for stock compose and simple SSH worlds; at ≥16 GB/instance and full recipe spin-up, LXD’s RAM/boot wins rarely justify nested-Docker complexity.
