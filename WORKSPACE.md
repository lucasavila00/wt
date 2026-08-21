# Workspace

The Cargo workspace is organized by product. Shared crates require production
consumers from at least two products.

```text
crates/products/wt/{client,control-protocol,server,retained-worlds}
crates/products/wt/{devcontainer-guest-tools,codex-integration,server-installer}
crates/products/gh-actions-runner/service
crates/products/agent-git-gateway/{gateway,git-hosting}
crates/products/git-proxy/{service,installer}
crates/shared/{libvirt-kvm,workload-registry,git-smart-protocol,installer-support}
crates/tests/end-to-end
```

`wt-retained-worlds` owns both retained-world lifecycles. `wt-libvirt-kvm`
owns the supported machine implementation and transport contract.
`wt-workload-registry` owns all persisted workload state and capacity.

Useful commands:

```text
cargo check --workspace
cargo run -p wt-client -- --help
cargo run -p wt-server -- --help
cargo run -p wt-server-installer -- --help
make install-git-server CONFIG=path/to/config.toml
```

Development setup and required checks: [Development](./DEVELOPMENT.md).
