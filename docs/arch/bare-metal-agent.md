# Libvirt/KVM backend

Parent: [architecture](./README.md). Implementation:
[`wt-libvirt`](../../crates/wt-libvirt/).

## Contract

| Piece | Choice |
|-------|--------|
| Isolation | One KVM guest per world |
| Host | Ubuntu 24.04 amd64 with KVM |
| Guest | Prepared Ubuntu 24.04 image |
| Provisioning | QEMU guest agent |
| Network | Libvirt DHCP identified by guest MAC |
| Access | OpenSSH as non-root user `wt` |
| Runtime | Docker, Buildx, Compose, configured tmux/Byobu frontend, Dev Container CLI |

No KVM emulation fallback. No public control-plane listener.

## Provision

1. Create a qcow2 overlay and cloud-init seed.
2. Boot with unique machine, network, and SSH identities.
3. Wait for QEMU, Docker, networking, and SSH.
4. Verify the endpoint's SSH keys against keys read through QEMU.
5. Clone the SSH Git source into `/workspace`.
6. Install the encrypted Git identity and known-hosts bundle.
7. Run the repository's stock devcontainer.
8. Install the configured persistent app-session helpers.
9. Record the guest IP and SSH keys; mark the world `Running`.

One deadline covers clone and recipe startup. Any create failure removes the
domain and files.

An endpoint key mismatch is an error. WT reports the IP, fingerprints, and any
other WT domain using that IP. It never removes the conflicting domain
automatically.

## Image

`wt-server-setup` builds and verifies the golden image. The image contains no
reusable machine ID or SSH host keys. Its manifest pins source, config, recipe,
packages, CLI version, and image digest.

## Registry cache

`wt-server-setup` runs a pinned caching proxy on the libvirt bridge. Worlds trust
its private CA. See [registry-cache.md](./registry-cache.md).
