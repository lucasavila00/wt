# Workspace

The repository is one Cargo workspace plus embedded POSIX shell installers in
`assets/`. Rust packages are private and share their
version, Rust edition, license, and common dependencies from the root
`Cargo.toml`.

## Packages

| Package | Kind | Role |
|---------|------|------|
| [`wt-api`](./crates/wt-api/) | Library | Control-plane JSON types |
| [`wt-cli`](./crates/wt-cli/) | Binary `wt` | Client CLI |
| [`wt-command`](./crates/wt-command/) | Library | Process command builder |
| [`wt-guest`](./crates/wt-guest/) | Binaries | Guest session and SSH helpers |
| [`wt-libvirt`](./crates/wt-libvirt/) | Library | Libvirt/KVM backend |
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
