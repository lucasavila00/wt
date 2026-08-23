# Workspace

The Cargo workspace is organized by product. Shared crates require production
consumers from at least two products.

```text
crates/products/wt/{client,control-protocol,server,guest}
crates/products/wt/{codex-integration,server-installer}
crates/products/agent-tools/{gateway,tools}
crates/products/git-proxy/{service,installer}
crates/shared/{libvirt-kvm,workload-registry,git-smart-protocol,installer-support}
crates/tests/end-to-end
```

`wt-guest` owns guest lifecycle. `wt-libvirt-kvm`
owns the supported machine implementation and transport contract.
`wt-workload-registry` owns all persisted workload state and capacity.

Useful commands:

```text
cargo check --workspace
cargo run -p wt-client -- --help
cargo run -p wt-server-installer --bin wts -- --help
cargo run -p wt-client --features guest --bin wtg -- --help
make install-git-server CONFIG=path/to/config.toml
```
