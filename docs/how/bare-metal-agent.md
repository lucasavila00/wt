# Libvirt/KVM backend

Each world is one KVM guest with its own network, Docker daemon, checkout, and
devcontainer.

| Layer | Implementation |
|-------|----------------|
| Host | Ubuntu 24.04 amd64 with KVM |
| VM lifecycle | Libvirt and QEMU |
| Disk | Golden-image-backed qcow2 overlay |
| Initial setup | Cloud-init |
| Readiness and file injection | QEMU guest agent |
| Network | Libvirt DHCP, identified by MAC address |
| Access | OpenSSH as non-root user `wt` |

## Create

1. Create the overlay and cloud-init seed.
2. Boot with unique machine, network, and SSH identities.
3. Wait for guest agent, Docker, network, and guest SSH.
4. Clone the SSH Git source into `/workspace`.
5. Install checkout-local Git credentials and trust.
6. Start the stock devcontainer with the pinned SSHD feature and app identity.
7. Install and verify session and proxy helpers.
8. Verify guest and app SSH; mark the world `running`.

A create failure removes the domain and world files. Endpoint host-key mismatch
is an error and never removes another domain automatically.

`wt-server-setup` builds and verifies the golden image. Its provenance pins the
source image, build config, recipe, packages, Dev Container CLI, and result
digest. The image contains no reusable machine ID or SSH host keys.

Implementation: [`wt-libvirt`](../../crates/wt-libvirt/). Parent:
[How WT works](./README.md).
