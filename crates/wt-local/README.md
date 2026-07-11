# wt-local

Single-site server: **control-plane API + embedded bare-metal worker**.

Runs on the hypervisor. The CLI reaches it **via SSH** to this host (not a public control-plane URL). API is loopback/stdio/socket-oriented; owner = connecting SSH user.

## Role

| Does | Does not |
|------|----------|
| Serve control-plane ops to authenticated SSH users | Require separate bearer-token product for bare metal |
| Embedded worker (stub → libvirt) | Multi-node fleet by itself |
| Local inventory + domain reconcile | |

Design: [docs/arch/control-plane.md](../../docs/arch/control-plane.md), [docs/arch/cli.md](../../docs/arch/cli.md), [docs/arch/bare-metal-agent.md](../../docs/arch/bare-metal-agent.md).

## Run

```text
cargo run -p wt-local
# CLI on Mac: context ssh = user@this-host
```

## Status

Topology only; provision not implemented.
