# wt

Named parallel instances of an existing Docker/devcontainer recipe. Mac is cockpit (`wt` + `ssh`); worlds run behind a control-plane URL.

**Plan:** [docs/plan.md](./docs/plan.md)  
**Architecture:** [docs/arch/README.md](./docs/arch/README.md)  
**Implementation eras:** [docs/impl/README.md](./docs/impl/README.md)

## Workspace topology

```text
Cargo.toml                 workspace root
crates/
  wt-api/                  shared control-plane HTTP/JSON types (library)
  wt/                      CLI binary (`wt`)
  wt-local/                v1 site server (`wt-local` = control plane + embedded worker)
docs/
  plan.md
  arch/
  impl/
  plan-reasoning/
```

| Package | Kind | Purpose |
|---------|------|---------|
| [`wt-api`](./crates/wt-api/) | lib | Shared control-plane API types / enums |
| [`wt`](./crates/wt/) | bin | Local CLI — client + print SSH Host snippets |
| [`wt-local`](./crates/wt-local/) | bin | **v1 server** — control plane + embedded bare-metal worker |

### Deferred (not in workspace yet)

| Binary | Role |
|--------|------|
| `wt-control-plane` | Multi-node control plane only; workers report in |
| `wt-worker` | Worker only (libvirt / later k8s) |

**Language:** Rust only.  
**v1:** `wt` + `wt-local`. k8s / multi-node binaries later.

## Build

```text
cargo check --workspace
cargo run -p wt
cargo run -p wt-local
```

Implementation not started — packages are topology + READMEs only.
