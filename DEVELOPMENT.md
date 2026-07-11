# Development

Local target: Ubuntu 24.04, amd64.

## Base

- Rust stable, Cargo, rustfmt, Clippy via rustup.
- Ubuntu packages:

```text
sudo apt update && sudo apt install -y build-essential pkg-config git curl openssh-client cpu-checker qemu-system-x86 qemu-utils libvirt-daemon-system libvirt-clients cloud-image-utils libguestfs-tools ovmf libvirt-dev
```

The injected-worker integration tests do not use libvirt/KVM.

For the complete local site setup, run:

```text
scripts/install-site
```

## Libvirt/KVM

The real VM backend is libvirt/KVM. KVM is required.

QEMU supplies the userspace VM process and virtual devices. KVM executes guest CPU instructions in hardware. Both are part of the same libvirt/KVM backend.

Required:

- CPU virtualization enabled in host firmware.
- `kvm-ok` succeeds.
- `/dev/kvm` exists.
- Development user belongs to `kvm` and `libvirt`.

Create the local image directory:

```text
sudo apt install -y libguestfs-tools
sudo install -d -o "$USER" -g libvirt -m 2770 /var/lib/libvirt/images/wt
```

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

Prepare the Docker-ready image once:

```text
scripts/prepare-image
```

This creates:

```text
imgs/wt-ubuntu-24.04-amd64.qcow2
```

Install it at the site path:

```text
sudo install -d -o root -g root -m 0755 /var/lib/wt/images
sudo install -o root -g root -m 0644 imgs/wt-ubuntu-24.04-amd64.qcow2 /var/lib/wt/images/wt-ubuntu-24.04-amd64.qcow2
```

`wt-libvirt` stages the installed image in its world directory and creates per-world qcow2 overlays from it. Docker is not installed during world creation.

Run the real acceptance test:

```text
cargo build --workspace && cargo test -p wt-integration-tests --test kvm_e2e -- --ignored
```

## Runtime

- Writable libvirt image directory for the staged image and qcow2 overlays.
- Test libvirt network with DHCP.
- Permission to use the system libvirt socket.
- Disk and memory for one test guest.

KVM needs CPU virtualization support, host firmware support, `/dev/kvm`, and user access to it.

## Still to decide

- Test pool and network names.
- Guest CPU, memory, and disk defaults.

Installation and host configuration instructions land after testing the setup on the local workstation.
