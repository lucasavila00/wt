# wt

Named parallel instances of an existing Docker/devcontainer recipe. The Mac is cockpit only (`wt` + `ssh`); worlds run on a remote site server.

| Doc | |
|-----|--|
| [docs/plan.md](./docs/plan.md) | Product plan |
| [docs/arch/](./docs/arch/README.md) | Architecture |
| [docs/impl/](./docs/impl/README.md) | Implementation eras |
| [docs/plan-reasoning/](./docs/plan-reasoning/) | Background notes |

## Workspace

```text
crates/
  wt-api/      shared control-plane HTTP/JSON types (library)
  wt-cli/      CLI package (binary name: `wt`)
  wt-local/    site server (`wt-local` = control plane + embedded worker)
```

| Package | Kind | Role |
|---------|------|------|
| [`wt-api`](./crates/wt-api/) | lib | Control-plane wire types |
| [`wt-cli`](./crates/wt-cli/) | bin `wt` | CLI — SSH contexts, API over SSH, world Host sync |
| [`wt-local`](./crates/wt-local/) | bin | Site server on hypervisor — plane + worker (reached via SSH) |

Out of the workspace until multi-node is in scope: `wt-control-plane`, `wt-worker`.

## Build

```text
cargo check --workspace
cargo run -p wt-cli
cargo run -p wt-local
```

Implementation of commands and provision is not started yet (topology + docs only).
