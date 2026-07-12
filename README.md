# wt

Named parallel instances of an existing Docker/devcontainer recipe. The client is
a thin cockpit (`wt` + `ssh`); worlds run on configured Ubuntu/KVM servers.

| Doc | |
|-----|--|
| [docs/plan.md](./docs/plan.md) | Product plan |
| [docs/arch/](./docs/arch/README.md) | Architecture |
| [docs/plan-reasoning/](./docs/plan-reasoning/) | Background notes |
| [DEVELOPMENT.md](./DEVELOPMENT.md) | Local setup, tests, and operator smoke test |

## Workspace

```text
crates/
  wt-api/      shared control-plane JSON types (library)
  wt-cli/      CLI package (binary name: `wt`)
  wt-guest/    host-built programs injected into guests
  wt-libvirt/  production libvirt/KVM world backend
  wt-server/    server helper + registry + control-plane service
  wt-server-setup/    Ubuntu/KVM server installer
  wt-integration-tests/  injected + real-system tests
```

| Package | Kind | Role |
|---------|------|------|
| [`wt-api`](./crates/wt-api/) | lib | Control-plane wire types |
| [`wt-cli`](./crates/wt-cli/) | bin `wt` | Context-aware CLI — new, ls, rm, sync |
| [`wt-guest`](./crates/wt-guest/) | bins | Host-built app session and SSH proxy helpers injected into guests |
| [`wt-libvirt`](./crates/wt-libvirt/) | lib | Libvirt/KVM world lifecycle |
| [`wt-server`](./crates/wt-server/) | bin | Server helper — registry + instance service + embedded backend |
| [`wt-server-setup`](./crates/wt-server-setup/) | bin | Strict Ubuntu/KVM server installation and golden image build |
| [`wt-integration-tests`](./crates/wt-integration-tests/) | tests | Injected service tests + libvirt/KVM acceptance test |

## Build

```text
cargo check --workspace
cargo run -p wt-cli
cargo run -p wt-server
```

Client configuration and operator usage are documented in
[`wt-cli`](./crates/wt-cli/README.md) and [DEVELOPMENT.md](./DEVELOPMENT.md).
