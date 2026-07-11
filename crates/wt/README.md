# wt (CLI)

Local **cockpit** binary: talk to an agent, maintain `~/.ssh/config` Host entries. No Docker.

## Role

| Does | Does not |
|------|----------|
| `new` / `ls` / `rm` against agent HTTP API | Run compose, libvirt, or clones |
| **Print** SSH `Host` snippets (auto-edit later, when stable) | Replace stock `ssh` for daily enter |
| Use [`wt-api`](../wt-api/) types | Own long-term instance truth (agent does) |

Design: [docs/arch/cli.md](../../docs/arch/cli.md).

## Binary

```text
cargo run -p wt -- --help   # once implemented
```

Install name: `wt`.

## Dependencies (planned)

- `wt-api` — shared types  
- HTTP client, CLI parser, config/SSH file editing — not wired yet  

## Status

Topology only — no commands implemented yet.
