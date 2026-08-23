# ADR 0018: Own the guest foundation in the golden image

- Status: Accepted
- Date: 2026-08-20

The golden image owns the guest foundation:

- `wt:wt` at UID/GID `1001:1001` with home `/home/wt`;
- the shared Byobu/tmux profile;
- Codex and the WT access, Git author, agent tool, and Codex mount helpers;
- a strict image-build result marker recording the identity contract.

`wt-guest` validates this foundation and applies per-world SSH keys,
Git author state, gateway access, and Codex mounts. It does not create or repair
a missing image contract. Provisioning has no checkpoints; remove a failed
world and create a fresh disk.

The reusable image contains no reusable SSH host keys. Provisioning generates
per-world keys and proves the one-use readiness login before removing it.
Replacing the golden image affects only newly created worlds.
