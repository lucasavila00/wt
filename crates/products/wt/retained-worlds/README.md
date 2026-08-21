# wt-retained-worlds

Shared guest contracts and provisioning operations for retained worlds.

It owns the fixed guest identity, access and Git helper paths, SSH host-key
handling, and the common provisioning flow for guest access, Git author state,
agent tooling, and Codex mounts.

Devcontainer- and host-specific lifecycle decisions are implemented in this
crate's product-owned modules.

System boundary: [Architecture](../../../../docs/internals/architecture.md).
