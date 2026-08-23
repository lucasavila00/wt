# wt-guest

Guest contracts and provisioning operations for guests.

It owns the fixed guest identity, access and Git helper paths, SSH host-key
handling, and the common provisioning flow for guest access, Git author state,
agent tooling, and Codex mounts.

Guest lifecycle decisions are implemented in this crate's product-owned module.
