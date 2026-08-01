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
3. `wt-provider` clones the SSH Git source, installs checkout-local credentials,
   starts the stock devcontainer, and installs the session and proxy helpers.
4. `wt-provider` verifies guest and app SSH and returns the running world.

A create failure removes the domain and world files. Endpoint host-key mismatch
is an error and never removes another domain automatically.

## Fork

1. The registry reserves two new writable disk heads below the source's current
   head and makes the shared point immutable.
2. Libvirt uses the QEMU guest agent to quiesce the running source and performs
   an atomic, disk-only external snapshot to its new head. WT checks that the
   guest is thawed on both success and failure paths.
3. The fork boots from the sibling head without a virtual NIC. Through
   the guest agent, WT replaces the hostname, machine ID, guest SSH host keys,
   app SSH host key, and guest-held app session key.
4. WT attaches the NIC, waits for DHCP, and verifies guest and app SSH before
   recording the fork as running.

The source is never stopped. The fork receives persistent disk state but no RAM
or processes. A post-pivot failure keeps the source on its new writable head;
a pre-pivot failure rolls the graph reservation back.

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
