# Libvirt/KVM backend

Production world backend in [`wt-libvirt`](../../crates/wt-libvirt/). Parent:
[architecture](./README.md).

## World

| Piece | Choice |
|-------|--------|
| Isolation | KVM guest per world |
| Image | Prepared Ubuntu 24.04 amd64 golden image |
| Runtime | Docker Engine, Compose v2, and pinned Dev Container CLI |
| Provisioning | QEMU guest agent through libvirt |
| Network | Configured libvirt network with a guest IP |
| Interactive access | OpenSSH to fixed non-root user `wt` |
| Trust | Trusted server and trusted world/container |

KVM is required. There is no CPU-emulation fallback.

## Provision

```text
1. Validate the prepared image and /dev/kvm
2. Create the qcow2 overlay and cloud-init seed
3. Inject guest login keys and generate a unique SSH host identity
4. Define and start the KVM domain through libvirt
5. Wait for the QEMU guest agent, Docker, Compose, and sshd
6. Clone the SSH Git source and check out the requested ref in /workspace
7. Install the checkout-local Git identity and host-trust bundle
8. Run devcontainer up --workspace-folder /workspace
9. Inject and verify the wt-app-shell guest helper
10. Record the guest IP and public SSH host keys
11. Report Running
```

The QEMU guest agent is the provisioning channel. Guest commands receive source
and ref values as arguments rather than interpolated shell text. One configured
recipe deadline covers clone, checkout, and devcontainer startup. Output streams
to helper stderr, while failures retain bounded phase and exit details.

The checkout is never mounted or exported to the server. The configured Git
identity and known-hosts data are installed under `/workspace/.git/wt`, where Git
in both the guest and the stock devcontainer can use them. The server, world, and
devcontainer are therefore one trusted credential boundary until deletion.

Guest helpers are compiled on the Ubuntu server, installed with the WT
binaries, and copied into worlds through the guest agent. The golden image does
not contain a Rust toolchain.

Any create failure removes the domain and world directory. Destroy stops and
undefines the domain, including NVRAM, and removes its files.

## Image

`scripts/prepare-image` builds a golden image containing Docker Engine, Compose
v2, QEMU guest agent, Git, OpenSSH server, and the pinned Dev Container CLI. The
manifest records the complete recipe and provenance. World creation does not
install packages, and golden images do not contain reusable SSH host keys.
