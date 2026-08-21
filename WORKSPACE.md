# Workspace

The repository is one Cargo workspace plus [shell assets](./assets/README.md).
Rust packages are private and share their version, Rust edition, license, and
common dependencies from the root `Cargo.toml`.

## Packages

| Package | Kind | Role |
|---------|------|------|
| [`wt-control-protocol`](./crates/products/wt/control-protocol/) | Library | Control-plane JSON types |
| [`wt-client`](./crates/products/wt/client/) | Binary `wt` | Client CLI |
| [`wt-command`](./crates/wt-command/) | Library | Process command builder |
| [`wt-devcontainer`](./crates/wt-devcontainer/) | Library | Devcontainer world lifecycle and provisioning |
| [`wt-git-smart-protocol`](./crates/shared/git-smart-protocol/) | Library | Shared Git transport and write policy |
| [`wt-agent-git-gateway`](./crates/wt-agent-git-gateway/) | Library and binaries | WT world Git access and provider CLI |
| [`wt-git-proxy`](./crates/products/git-proxy/service/) | Binary | Standalone OpenSSH Git proxy |
| [`wt-devcontainer-guest-tools`](./crates/wt-devcontainer-guest-tools/) | Binaries | Devcontainer session and SSH helpers |
| [`wt-gh-actions-runner`](./crates/wt-gh-actions-runner/) | Library | Ephemeral GitHub Actions world lifecycle |
| [`wt-host`](./crates/wt-host/) | Library | Raw Ubuntu host world lifecycle |
| [`wt-libvirt-kvm`](./crates/shared/libvirt-kvm/) | Library | Libvirt/KVM backend |
| [`wt-provider`](./crates/wt-provider/) | Library | Shared machine-provider contracts |
| [`wt-workload-registry`](./crates/shared/workload-registry/) | Library | Shared guest registry and capacity admission |
| [`wt-server`](./crates/wt-server/) | Binary | Server API, registry, and jobs |
| [`wt-server-installer`](./crates/products/wt/server-installer/) | Binary | Server installer and image builder |
| [`wt-end-to-end-tests`](./crates/tests/end-to-end/) | Tests | Cross-crate and KVM tests |

## Commands

```text
cargo check --workspace
cargo run -p wt-client -- --help
cargo run -p wt-server -- --help
cargo run -p wt-server-installer -- --help
make install-git-server
```

Development setup and required checks: [Development](./DEVELOPMENT.md).
