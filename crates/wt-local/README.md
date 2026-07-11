# wt-local

Single-site server: **control-plane API + embedded bare-metal worker**.

Runs on the hypervisor. The CLI runs a **remote command over SSH** (JSON in/out); owner = SSH user. No public control-plane HTTP.

## Role

| Does | Does not |
|------|----------|
| Expose control-plane ops as an SSH-invoked helper (stdio JSON) | Require separate bearer-token product for bare metal |
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
