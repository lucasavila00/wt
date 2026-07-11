# Bare-metal worlds (one big server)

N instances on one fat host without port soup.  
Isolation: [isolation-without-port-overrides.md](./isolation-without-port-overrides.md). Deploy: [../arch/bare-metal-agent.md](../arch/bare-metal-agent.md) (`wt-server`).

## Constraints

- Per-instance network identity so stock `"3000:3000"` works N times  
- Trusted pool  
- Typical guest **≥16 GB** — app RAM dominates  
- Wall clock dominated by **clone + images + compose**, not empty-guest boot  

## Chosen approach

**KVM/libvirt guest per instance** via **`wt-server`**.

- Own IP + own Docker → stock compose  
- Clear `ssh <name>` target  
- Stable, transparent DX for compose authors  

At ≥16 GB/instance, guest OS overhead (~0.5–1 GB class) is minor.

## Alternatives (not the home path)

| Approach | Note |
|----------|------|
| One Docker, many IPs (macvlan) | Dense; weaker world boundary; Host story messier |
| LXD + Docker inside | Denser than KVM; nested Docker complexity |
| A cluster orchestrator on the server | Unnecessary for the implemented single-server KVM backend |

`wt-server` runs on the hypervisor and combines the server control plane with its
libvirt worker.

## One-line summary

**On one fat trusted host, KVM-per-instance under `wt-server` for stock compose and simple SSH worlds.**
