# Libvirt/KVM backend

Each world is one KVM guest with its own network, Docker daemon, checkout, and
devcontainer.

| Layer | Implementation |
|-------|----------------|
| Host | Ubuntu 24.04 amd64 with KVM |
| VM lifecycle | Libvirt and QEMU |
| Disk | Registry-owned graph of golden-image-backed qcow2 nodes |
| Machine bootstrap | Cloud-init installs and activates the QEMU guest agent |
| Provisioning transport | QEMU guest agent through `wt-provider` |
| Network | Libvirt DHCP, identified by MAC address |
| Access | OpenSSH as non-root user `wt` |

## Create

1. `wt-libvirt` validates the provider ID, creates the overlay, seed, and
   domain, then waits for the guest agent and current DHCP address.
2. `wt-provider` verifies and bootstraps Ubuntu, installs the locked toolchain,
   and configures the user, workspace, registry trust, and guest SSH identity.
3. `wt-provider` installs the gateway relay and session helpers, then returns a
   guest ready for setup.
4. The persistent setup session clones through the gateway, starts the stock
   devcontainer, and verifies app SSH.

A create failure removes the domain and world files. Endpoint host-key mismatch
is an error and never removes another domain automatically.

`wt-server-setup` builds and verifies the golden image. Its provenance pins the
source image, build config, recipe, packages, Dev Container CLI, and result
digest. The setup image recipe and runtime provisioner consume the same package
policy; the complete installed package set and every resolved version must
match before the image is reused. Machine bootstrap and world provisioning
still install or verify their requirements so correctness does not depend on
the golden image. The image contains no reusable machine ID or SSH host keys.

Implementations: [`wt-provider`](../../crates/wt-provider/) and
[`wt-libvirt`](../../crates/wt-libvirt/). Parent:
[How WT works](./README.md).
