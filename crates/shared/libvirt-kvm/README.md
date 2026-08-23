# wt-libvirt-kvm

Production libvirt/KVM backend.

## Owns

- Domain, network, and qcow2 overlay disk lifecycle.
- Guest-agent readiness and bounded guest transport.
- Machine inspection, start, stop, disk usage, and deletion.

The guest crate owns provisioning and readiness after the machine is
available.
