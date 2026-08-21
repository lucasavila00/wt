# wt-retained

Shared guest contracts and provisioning operations for retained worlds.

It owns the fixed guest identity, access and Git helper paths, SSH host-key
handling, and the common provisioning flow for guest access, Git author state,
agent Git, and Codex mounts.

Devcontainer- and host-specific lifecycle decisions stay in
`wt-devcontainer` and `wt-host`.

System boundary: [Architecture](../../docs/internals/architecture.md).
