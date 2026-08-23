# Provider boundaries

`wt-libvirt-kvm` owns the only production machine implementation:

- machine resources and disk identity;
- create, inspect, start, stop, disk usage, and delete operations;
- bounded guest command, capture, and file-write transport.

It uses libvirt/KVM and the QEMU guest agent and does not know Git grants or
world access policy. `wt-host-world::host` composes a machine with the
golden-image contract, login preparation, SSH proof, Git author, agent tooling,
and Codex mounts.

`wt-server` owns the world operation and persists its backend and disk state.
A second provider abstraction is not maintained until a second production
provider exists.
