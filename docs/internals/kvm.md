# KVM and NoCloud

`wt-libvirt` owns machine creation, inspection, start, and deletion. It creates
qcow2 overlays, libvirt domains, NoCloud seed images, network identity, and QEMU
guest-agent transport.

Every machine seed contains separate files:

- `user-data`: application input;
- `vendor-data`: WT-owned bootstrap data;
- `meta-data`: unique instance and host name;
- `network-config`: DHCP keyed by the unique MAC address.

Devcontainer and GitHub CI machine code uses empty application user/vendor data
and performs bootstrap through its own lifecycle. Host machines put the
operator recipe in user-data and the `wt` login in vendor-data.

## Images

Retained worlds use two installed images built from the same pinned Ubuntu
source:

- devcontainer: Docker, Git, Dev Container CLI, Byobu, and tmux;
- host: upstream Ubuntu plus OpenSSH, QEMU guest agent, Byobu, tmux, and shared
  terminal assets.

`wt-server-setup` builds each image independently through one KVM builder.
Shared recipes live in `assets/world/shared`; kind recipes live beside their
kind. [ADR 0027](../adr/0027-build-images-in-kvm.md) records the build contract.

Each image has its own provenance manifest and checksum. Image paths cannot be
the same file. A world disk cannot be smaller than its backing image; the
provider rejects it before creating the overlay. Per-world writable disks and
SSH host keys remain unique.

## Readiness

Libvirt waits for QEMU guest agent and an IPv4 address. Expected failed agent
polls are silent; a timeout names the domain and reports the last libvirt error.
The kind lifecycle then defines readiness:

- devcontainer verifies guest setup and app SSH;
- host waits for cloud-init, pins host SSH, proves `wt` login, and removes its
  one-use probe key;
- GitHub CI waits for its runner process and job lifecycle.

Stopped retained guests keep their disk and identity. Missing files, mismatched
identity, or partial libvirt state fail closed.
