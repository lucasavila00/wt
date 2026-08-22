# Workspace

The Cargo workspace is organized by product. Shared crates require production
consumers from at least two products.

```text
crates/products/wt/{client,control-protocol,server,retained-worlds}
crates/products/wt/{codex-integration,server-installer}
crates/products/agent-tools/{gateway,tools}
crates/products/git-proxy/{service,installer}
crates/shared/{libvirt-kvm,workload-registry,git-smart-protocol,installer-support}
crates/tests/end-to-end
```

`wt-retained-worlds` owns retained host lifecycle. `wt-libvirt-kvm`
owns the supported machine implementation and transport contract.
`wt-workload-registry` owns all persisted workload state and capacity.

Useful commands:

```text
scripts/cargo check --workspace
scripts/cargo run -p wt-client -- --help
scripts/cargo run -p wt-server -- --help
scripts/cargo run -p wt-server-installer -- --help
make install-git-server CONFIG=path/to/config.toml
```
