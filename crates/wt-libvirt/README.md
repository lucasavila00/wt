# wt-libvirt

Production libvirt/KVM world backend.

- KVM required.
- Libvirt owns domain lifecycle and inventory.
- QEMU supplies the userspace VM process and qcow2 support under KVM.
- Cloud-init injects per-world identity.
- QEMU guest agent verifies Docker Engine and Compose readiness through libvirt.
- Era 1.5 adds SSH Git clone, ref checkout, persistent checkout-local Git credentials, guest SSH, and devcontainer readiness.

Used by [`wt-local`](../wt-local/). Real-system tests live in [`wt-integration-tests`](../wt-integration-tests/).

## Worker layout

- `worker.rs` orchestrates create, inspect, and destroy against libvirt.
- `worker/world.rs` renders cloud-init and domain XML and names per-world files.
- `worker/guest_agent.rs` executes commands and writes files through QEMU, with one deadline and bounded error output.
- `worker/git.rs` handles the interactive SSH identity, clone/ref selection, and the `.git/wt` credential bundle shared with the devcontainer.

Create prepares an overlay and cloud-init seed, starts the KVM domain, waits for QEMU and SSH readiness, clones the requested revision, starts the stock devcontainer, and only then reports `Running`. Any create failure removes both the domain and its world directory.
