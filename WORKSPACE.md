# Workspace

The repository is one Cargo workspace plus [shell assets](./assets/README.md).
Rust packages are private and share their version, Rust edition, license, and
common dependencies from the root `Cargo.toml`.

## Packages

| Package | Kind | Role |
|---------|------|------|
| [`wt-api`](./crates/wt-api/) | Library | Control-plane JSON types |
| [`wt-cli`](./crates/wt-cli/) | Binary `wt` | Client CLI |
| [`wt-command`](./crates/wt-command/) | Library | Process command builder |
| [`wt-devcontainer`](./crates/wt-devcontainer/) | Library | Devcontainer world lifecycle and provisioning |
| [`wt-devcontainer-git`](./crates/wt-devcontainer-git/) | Library and binaries | Scoped Git transport for devcontainer worlds |
| [`wt-devcontainer-guest`](./crates/wt-devcontainer-guest/) | Binaries | Devcontainer session and SSH helpers |
| [`wt-github-ci`](./crates/wt-github-ci/) | Library | Ephemeral GitHub Actions world lifecycle |
| [`wt-host`](./crates/wt-host/) | Library | Raw Ubuntu host world lifecycle |
| [`wt-libvirt`](./crates/wt-libvirt/) | Library | Libvirt/KVM backend |
| [`wt-provider`](./crates/wt-provider/) | Library | Shared machine-provider contracts |
| [`wt-registry`](./crates/wt-registry/) | Library | Shared guest registry and capacity admission |
| [`wt-server`](./crates/wt-server/) | Binary | Server API, registry, and jobs |
| [`wt-server-setup`](./crates/wt-server-setup/) | Binary | Server installer and image builder |
| [`wt-integration-tests`](./crates/wt-integration-tests/) | Tests | Cross-crate and KVM tests |

## Commands

```text
cargo check --workspace
cargo run -p wt-cli -- --help
cargo run -p wt-server -- --help
cargo run -p wt-server-setup -- --help
```

Development setup and required checks: [Development](./DEVELOPMENT.md).
