# wt

WT creates named, parallel devcontainer environments on Ubuntu/KVM servers.
Each world has its own VM, Git checkout, Docker daemon, network, and stock
devcontainer recipe. The client uses `wt` to manage worlds and OpenSSH to enter
them.

```text
wt new git@github.com:org/repo.git lab.repo-feature
wt ls
ssh lab.repo-feature
wt rm lab.repo-feature
```

## Documentation

| Document | Contents |
|----------|----------|
| [Getting started](./GETTING-STARTED.md) | Install and use WT |
| [Architecture](./docs/arch/README.md) | Components, data flow, and boundaries |
| [Product](./docs/product.md) | Scope and constraints |
| [Development](./DEVELOPMENT.md) | Build, test, and local KVM workflow |

## Packages

| Package | Role |
|---------|------|
| [`wt-api`](./crates/wt-api/) | Control-plane JSON types |
| [`wt-cli`](./crates/wt-cli/) | `wt` client |
| [`wt-command`](./crates/wt-command/) | Process command builder |
| [`wt-guest`](./crates/wt-guest/) | Guest session and SSH helpers |
| [`wt-libvirt`](./crates/wt-libvirt/) | Libvirt/KVM backend |
| [`wt-server`](./crates/wt-server/) | Server API, registry, and jobs |
| [`wt-server-setup`](./crates/wt-server-setup/) | Server installer and image builder |
| [`wt-integration-tests`](./crates/wt-integration-tests/) | Cross-crate and KVM tests |

## Build

```text
cargo check --workspace
cargo run -p wt-cli -- --help
```
