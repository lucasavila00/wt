# wt

Named parallel instances of an existing Docker/devcontainer recipe. The Mac is cockpit only (`wt` + `ssh`); worlds run on a remote site server.

| Doc | |
|-----|--|
| [docs/plan.md](./docs/plan.md) | Product plan |
| [docs/arch/](./docs/arch/README.md) | Architecture |
| [docs/impl/](./docs/impl/README.md) | Implementation eras |
| [docs/plan-reasoning/](./docs/plan-reasoning/) | Background notes |
| [DEVELOPMENT.md](./DEVELOPMENT.md) | Local development prerequisites |
| [TESTS.md](./TESTS.md) | Test prerequisites and commands |

## Workspace

```text
crates/
  wt-api/      shared control-plane JSON types (library)
  wt-cli/      CLI package (binary name: `wt`)
  wt-libvirt/  production libvirt/KVM world backend
  wt-local/    site helper + registry + control-plane service
  wt-setup/    Ubuntu/KVM site installer
  wt-integration-tests/  injected + real-system tests
```

| Package | Kind | Role |
|---------|------|------|
| [`wt-api`](./crates/wt-api/) | lib | Control-plane wire types |
| [`wt-cli`](./crates/wt-cli/) | bin `wt` | Local CLI — new, ls, rm, sync, ssh |
| [`wt-libvirt`](./crates/wt-libvirt/) | lib | Libvirt/KVM world lifecycle |
| [`wt-local`](./crates/wt-local/) | bin | Local helper — registry + instance service + embedded backend |
| [`wt-setup`](./crates/wt-setup/) | bin | Strict Ubuntu/KVM site installation and golden image build |
| [`wt-integration-tests`](./crates/wt-integration-tests/) | tests | Injected service tests + libvirt/KVM acceptance test |

Out of the workspace until multi-node is in scope: `wt-control-plane`, `wt-worker`.

## Build

```text
cargo check --workspace
cargo run -p wt-cli
cargo run -p wt-local
```

Era 1.5 provides SSH-only Git/devcontainer provisioning and interactive guest access.
