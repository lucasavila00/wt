# wt (CLI)

Cockpit binary: control-plane client and **printed** SSH Host snippets. No Docker.

## Role

| Does | Does not |
|------|----------|
| `new` / `ls` / `rm` against control-plane API | Run compose or libvirt |
| Print SSH `Host` blocks | Replace stock `ssh` |
| Use [`wt-api`](../wt-api/) | Own server-side inventory |

Design: [docs/arch/cli.md](../../docs/arch/cli.md).  
Default server: [`wt-local`](../wt-local/).

## Run

```text
cargo run -p wt
```

## Status

Topology only; commands not implemented.
