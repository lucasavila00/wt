# Bare-metal worlds (one big server)

How to run **N instances** on **one** fat host without port soup.  
Context: [isolation-without-port-overrides.md](./isolation-without-port-overrides.md), [idealized-api.md](./idealized-api.md). **Plan:** [plan.md](../plan.md) — this is the **home / 1–2 server** provider path (not company k8s).

## Constraint reminder

- Need **per-instance network identity** so stock `"3000:3000"` works N times.
- **Trusted pool** only—not security sandboxing.
- Planning: **≥16 GB per instance** is normal; app RAM dominates.
- Create path pays **clone + images + compose/devcontainer up**; empty-world **boot is not the long pole**.

## Options

### 1. One Docker, many IPs (macvlan / host addresses)

One kernel, one Docker. Each instance publishes on **its** IP.

| + | − |
|---|---|
| Max density | One daemon; weaker world boundary |
| No guest | `ssh <name>` less natural; publish bind-IP may need tool-owned tweak |

### 2. LXD / Incus system containers

System container + Docker **inside**. Can be transparent to compose authors.

| + | − |
|---|---|
| Dense | Nested Docker footguns until golden profile is boring |

### 3. KVM / libvirt guests — **plan default for bare metal**

VM + own IP + own Docker. Closest to mini server.

| + | − |
|---|---|
| Most stable stock Docker/compose DX | ~0.5–1 GB OS tax (noise at ≥16 GB/instance) |
| Obvious `ssh <name>` → guest | Slightly worse density than LXD |

## RAM / boot (decided enough)

- At **≥16 GB/instance**, KVM vs LXD RAM difference **does not drive the choice**.  
- Faster LXD boot **does not matter**—recipe spin-up dominates.  
- Prefer **KVM** for simplicity and transparent DX ([plan.md](../plan.md)).

## Relation to k8s

- **Do not** put a solo home box through k8s/minikube just to run compose worlds—**overkill**.  
- Company horizontal scale = **k8s provider** (DinD pods), separate agent—see plan.  
- KubeVirt optional later; not required for the bare-metal path.

## Lean

| Priority | Choice |
|----------|--------|
| Home bare metal (plan) | **KVM/libvirt** guest per instance |
| Density later | LXD if willing to own nesting |
| Max pack | multi-IP Docker (escape hatch) |

**Transparency rule:** world = “small Linux with Docker.” Never special host compose for authors.

## Still open (ops detail)

- Warm pool size vs cold clone-from-template  
- IP assignment (bridge + DHCP vs static)  

## One-line summary

On one fat trusted host, **KVM-per-instance** for stock compose and simple SSH; leave k8s to the company provider, not the home box.
