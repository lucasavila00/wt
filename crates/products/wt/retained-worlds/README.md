# wt-retained-worlds

Guest contracts and provisioning operations for retained host worlds.

It owns the fixed guest identity, access and Git helper paths, SSH host-key
handling, and the common provisioning flow for guest access, Git author state,
agent tooling, and Codex mounts.

Host lifecycle decisions are implemented in this crate's product-owned module.

System boundary: [Architecture](../../../../docs/internals/architecture.md).
