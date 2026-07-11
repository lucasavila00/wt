# wt-local

Single-site server: **control-plane API + embedded bare-metal worker**.

Runs on the **site** host (remote hypervisor **or** the same Ubuntu workstation as the CLI). Invoked as a **helper command** (JSON in/out)—by `ssh … -- wt-local …` or direct local exec. Owner = SSH user or local OS user. No public control-plane HTTP.

## Role

| Does | Does not |
|------|----------|
| Expose control-plane ops as a CLI-spawned helper (stdio JSON) | Require separate bearer-token product for bare metal |
| Embedded libvirt worker | Multi-node fleet by itself |
| Local inventory + domain reconcile | |

Design: [docs/arch/control-plane.md](../../docs/arch/control-plane.md), [docs/arch/cli.md](../../docs/arch/cli.md), [docs/arch/bare-metal-agent.md](../../docs/arch/bare-metal-agent.md).

## Run

```text
cargo run -p wt-local
# remote:  context kind = bare_metal_ssh,  ssh = user@this-host
# local:   context kind = bare_metal_local (helper on PATH)
```

## Status

Topology only; provision not implemented.
