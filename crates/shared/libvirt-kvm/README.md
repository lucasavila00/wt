# wt-libvirt-kvm

Production libvirt/KVM backend.

## Owns

- Domain, network, and independent qcow2 disk lifecycle.
- Guest-agent readiness and bounded guest transport.
- Machine inspection, start, stop, disk usage, and deletion.

The retained-world crate owns provisioning and readiness after the machine is
available.

Lifecycle: [KVM](../../docs/internals/kvm.md).
