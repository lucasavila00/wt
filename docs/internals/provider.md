# Provider boundaries

`wt-libvirt-kvm` owns the only production machine implementation:

- machine resources, with a libvirt domain and world disk derived from the
  world UUID;
- create, inspect, start, stop, disk usage, and delete operations;
- bounded guest command, capture, and file-write transport.

It uses libvirt/KVM and the QEMU guest agent and does not know Git grants or
world access policy. `wt-guest::host` composes a machine with the
golden-image contract, login preparation, SSH proof, Git author, agent tooling,
and Codex mounts.

`wt-server` owns the world lifecycle and registry metadata. It never persists
provider-specific identifiers: libvirt derives those internally from the world
UUID. A second provider abstraction is not maintained until a second production
provider exists.
