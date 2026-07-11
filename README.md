# wt

Named parallel instances of an existing Docker/devcontainer recipe. The client is
a thin cockpit (`wt` + `ssh`); worlds run on configured Ubuntu/KVM sites.

| Doc | |
|-----|--|
| [docs/plan.md](./docs/plan.md) | Product plan |
| [docs/arch/](./docs/arch/README.md) | Architecture |
| [docs/plan-reasoning/](./docs/plan-reasoning/) | Background notes |
| [DEVELOPMENT.md](./DEVELOPMENT.md) | Local development prerequisites |
| [TESTS.md](./TESTS.md) | Test prerequisites and commands |
| [MANUAL-TESTS.md](./MANUAL-TESTS.md) | Copy-paste operator smoke test |

## Workspace

```text
crates/
  wt-api/      shared control-plane JSON types (library)
  wt-cli/      CLI package (binary name: `wt`)
  wt-guest/    host-built programs injected into guests
  wt-libvirt/  production libvirt/KVM world backend
  wt-local/    site helper + registry + control-plane service
  wt-local-setup/    Ubuntu/KVM local-site installer
  wt-integration-tests/  injected + real-system tests
```

| Package | Kind | Role |
|---------|------|------|
| [`wt-api`](./crates/wt-api/) | lib | Control-plane wire types |
| [`wt-cli`](./crates/wt-cli/) | bin `wt` | Context-aware CLI — new, ls, rm, sync, ssh |
| [`wt-guest`](./crates/wt-guest/) | bin `wt-app-shell` | Host-built devcontainer shell helper injected into guests |
| [`wt-libvirt`](./crates/wt-libvirt/) | lib | Libvirt/KVM world lifecycle |
| [`wt-local`](./crates/wt-local/) | bin | Local helper — registry + instance service + embedded backend |
| [`wt-local-setup`](./crates/wt-local-setup/) | bin | Strict Ubuntu/KVM local-site installation and golden image build |
| [`wt-integration-tests`](./crates/wt-integration-tests/) | tests | Injected service tests + libvirt/KVM acceptance test |

## Build

```text
cargo check --workspace
cargo run -p wt-cli
cargo run -p wt-local
```

Client configuration and operator usage are documented in
[`wt-cli`](./crates/wt-cli/README.md) and [MANUAL-TESTS.md](./MANUAL-TESTS.md).
