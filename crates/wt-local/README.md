# wt-local

**v1 site server:** control-plane HTTP API + **embedded** bare-metal worker (libvirt later) in **one process**.

This is what you run on the home hypervisor. The CLI (`wt`) points at this URL.

## Role

| Does | Does not |
|------|----------|
| Serve **control-plane** API for CLI (`new` / `ls` / `rm`) | Run on the Mac |
| Embed local **worker** (stub → libvirt) | Multi-node fleet |
| Reconcile inventory on this host (anti-zombie) | Be the future standalone control-plane-only binary |
| Stock recipe in guest (later eras) | Invent a new env format |

Design: [docs/arch/control-plane.md](../../docs/arch/control-plane.md), [docs/arch/bare-metal-agent.md](../../docs/arch/bare-metal-agent.md) (worker backend notes).

## Deferred binaries (not this crate)

| Future binary | Role |
|---------------|------|
| `wt-control-plane` | Aggregate-only control plane; workers report in; disposable RAM/redis |
| `wt-worker` | Worker-only (bare-metal or k8s); reports to control-plane URL |

Those stay out of the workspace until multi-node. Logic should live in libraries so `wt-local` stays a thin wire-up of control-plane + worker.

## Binary

```text
cargo run -p wt-local
# CLI:  wt --control-plane-url http://hypervisor:port …
```

## Status

Topology only — no provision pipeline implemented yet.
