# wt-libvirt

Production libvirt/KVM world backend.

- KVM required.
- Libvirt owns domain lifecycle and inventory.
- QEMU supplies the userspace VM process and qcow2 support under KVM.
- Cloud-init injects per-world identity.
- QEMU guest agent verifies Docker Engine and Compose readiness through libvirt.
- Era 1.5 adds SSH Git clone, ref checkout, persistent checkout-local Git credentials, guest SSH, and devcontainer readiness.

Used by [`wt-local`](../wt-local/). Real-system tests live in [`wt-integration-tests`](../wt-integration-tests/).
