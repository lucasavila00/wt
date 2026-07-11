# wt-local

Single-site server: **control-plane API + embedded bare-metal worker**.

Run on the hypervisor. Point `wt` at this process’s URL.

## Role

| Does | Does not |
|------|----------|
| Serve control-plane API | Run on the Mac |
| Embedded worker (stub → libvirt) | Multi-node fleet by itself |
| Local inventory + domain reconcile | |

Design: [docs/arch/control-plane.md](../../docs/arch/control-plane.md), [docs/arch/bare-metal-agent.md](../../docs/arch/bare-metal-agent.md).

Multi-node (not in workspace): `wt-control-plane`, `wt-worker` as separate processes reusing the same libraries.

## Run

```text
cargo run -p wt-local
```

## Status

Topology only; provision not implemented.
