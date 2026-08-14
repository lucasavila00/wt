# wt-libvirt

Production libvirt/KVM backend.

## Owns

- Domain, network, qcow2 overlay, and NoCloud seed lifecycle.
- Guest-agent readiness and bounded guest transport.
- Machine inspection, start, fork, and deletion.

World-kind crates own application provisioning and readiness after the machine
is available.

Lifecycle: [KVM and NoCloud](../../docs/internals/kvm.md).
