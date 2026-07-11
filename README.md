# wt

Named parallel instances of an existing Docker/devcontainer recipe. Mac is cockpit (`wt` + `ssh`); worlds run on a remote agent.

**Plan:** [docs/plan.md](./docs/plan.md)  
**Architecture:** [docs/arch/README.md](./docs/arch/README.md)

## Workspace topology

```text
Cargo.toml                 workspace root
crates/
  wt-api/                  shared HTTP/JSON types (library)
  wt/                      CLI binary (`wt`)
  wt-agent/                bare-metal agent binary (`wt-agent`, v1)
docs/
  plan.md
  arch/
  plan-reasoning/
```

| Package | Kind | Purpose |
|---------|------|---------|
| [`wt-api`](./crates/wt-api/) | lib | Shared API types / enums (serde later) |
| [`wt`](./crates/wt/) | bin | Local CLI — agent client + SSH Host map |
| [`wt-agent`](./crates/wt-agent/) | bin | Bare-metal libvirt worlds |

**Language:** Rust only (one type system for CLI + agent).  
**v1:** CLI + bare-metal agent. k8s agent is a future binary/crate, not present yet.

## Build

```text
cargo check --workspace
cargo run -p wt
cargo run -p wt-agent
```

Implementation not started — packages are topology + READMEs only.
