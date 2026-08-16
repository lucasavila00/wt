# wt-libvirt

Production libvirt/KVM backend.

## Owns

- Domain, network, independent qcow2 disk, and NoCloud seed lifecycle.
- Guest-agent readiness and bounded guest transport.
- Machine inspection, start, fork, and deletion.

World-kind crates own application provisioning and readiness after the machine
is available.

Lifecycle: [KVM and NoCloud](../../docs/internals/kvm.md).
