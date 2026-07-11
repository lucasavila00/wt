# wt-agent

**Bare-metal** agent (v1): HTTP API + libvirt guests as worlds, stock compose/devcontainer inside each guest.

## Role

| Does | Does not |
|------|----------|
| Create/list/destroy instances | Run on the Mac |
| libvirt VM lifecycle + bootstrap | k8s / DinD (later agent) |
| Clone + stock recipe in guest | Invent a new env format |
| Source of truth for instance state | |

Design: [docs/arch/bare-metal-agent.md](../../docs/arch/bare-metal-agent.md).  
k8s (later): [docs/arch/k8s-agent.md](../../docs/arch/k8s-agent.md) — not this crate until that iteration.

## Binary

```text
cargo run -p wt-agent   # once implemented; runs on hypervisor host
```

## Dependencies (planned)

- `wt-api` — shared HTTP types  
- HTTP server, async runtime, libvirt/`virsh`, local state store — not wired yet  

## Status

Topology only — no provision pipeline implemented yet.
