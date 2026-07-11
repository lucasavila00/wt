# Development

Local target: Ubuntu 24.04, amd64.

## Base

- Rust stable, Cargo, rustfmt, Clippy via rustup.
- Ubuntu packages:

```text
sudo apt update && sudo apt install -y build-essential pkg-config git curl openssh-client qemu-system-x86 qemu-utils libvirt-daemon-system libvirt-clients virtinst cloud-image-utils ovmf libvirt-dev
```

The injected-worker integration tests do not use QEMU/libvirt.

## QEMU/libvirt

`qemu-kvm` is not a package candidate on this Ubuntu 24.04 workstation. `qemu-system-x86` supports software emulation and KVM acceleration through `/dev/kvm`.

## Guest image

Tests and development scripts expect the Ubuntu 24.04 amd64 cloud image at:

```text
imgs/ubuntu-24.04-server-cloudimg-amd64.img
```

Download it from the repo root:

```text
mkdir -p imgs && curl -fL https://cloud-images.ubuntu.com/releases/24.04/release/ubuntu-24.04-server-cloudimg-amd64.img -o imgs/ubuntu-24.04-server-cloudimg-amd64.img
```

`imgs/` is gitignored. Keep the base image unchanged; create per-world qcow2 overlays from it.

## Runtime

- Test storage pool for qcow2 overlays.
- Test libvirt network with DHCP.
- SSH key injected into guests.
- Permission to use the system libvirt socket.
- Disk and memory for one test guest.

KVM is optional at first. It needs CPU virtualization support, host firmware support, `/dev/kvm`, and user access to it.

## Still to decide

- Test pool and network names.
- Rust libvirt bindings vs `virsh` subprocesses.
- Guest CPU, memory, and disk defaults.

Installation and host configuration instructions land after testing the setup on the local workstation.
