# Development

Target: Ubuntu 24.04 amd64.

## Prerequisites

- Rust stable, Cargo, rustfmt, Clippy via rustup.
- CPU virtualization enabled in firmware.
- KVM available.

Ubuntu packages:

```text
sudo apt update && sudo apt install -y build-essential pkg-config git curl cpu-checker qemu-system-x86 qemu-utils libvirt-daemon-system libvirt-clients virtinst cloud-image-utils libguestfs-tools ovmf libvirt-dev acl
```

Host access:

```text
sudo usermod -aG libvirt,kvm "$USER"
sudo install -d -o "$USER" -g kvm -m 2770 /var/lib/libvirt/images/wt
sudo setfacl -m u:libvirt-qemu:--x /var/lib/libvirt/images/wt
```

The ACL grants only the libvirt QEMU service account directory traversal. It
avoids virt-install path-access warnings without making world files searchable
by every local user. `scripts/install-server` enforces the same owner, group,
mode, and ACL for the configured `libvirt.worlds_dir`.

Log out and back in after changing groups. Then require:

```text
kvm-ok
test -r /dev/kvm -a -w /dev/kvm
```

No software-emulation fallback exists.

## Config

`config/wt-server.development.toml` is the development sample. Cargo does not embed or install it.

Review CPU, memory, disk, paths, URL, and SHA before use:

```text
cargo run -p wt-server-setup -- validate --config config/wt-server.development.toml
```

There are no runtime environment overrides. `wt-server` always reads
`/etc/wt/server.toml`.

## Source image

Expected cache path:

```text
imgs/ubuntu-24.04-server-cloudimg-amd64.img
```

`imgs/` is gitignored. Setup downloads the pinned image when absent. Manual download:

```text
mkdir -p imgs
curl -fL https://cloud-images.ubuntu.com/releases/noble/release-20260615/ubuntu-24.04-server-cloudimg-amd64.img -o imgs/ubuntu-24.04-server-cloudimg-amd64.img
printf '%s  %s\n' 5fa5b05e5ec239858c4531485d6023b0896448c2df7c63b34f8dae6ea6051a44 imgs/ubuntu-24.04-server-cloudimg-amd64.img | sha256sum --check
```

## Setup

Complete local setup:

```text
scripts/install-server --config config/wt-server.development.toml
```

Image only:

```text
scripts/prepare-image --config config/wt-server.development.toml
```

Integration-test image cache:

```text
scripts/prepare-test-image --config config/wt-server.development.toml
```

Run these scripts in an interactive terminal. They invoke `sudo` and may ask for the password.

Image construction boots a temporary KVM guest. Cloud-init installs Docker Engine, Docker Compose v2, and QEMU guest agent. The installer verifies readiness, syspreps the disk, writes a provenance manifest, then publishes the golden image.

Matching installed state is reused. Config, permissions, partial files, stale build state, or image provenance drift is an error.

The test-image command creates a separate qcow2 backing image with the existing
jsdev Compose images preloaded. Prepare it once before running the workspace
tests, and prepare it again after rebuilding the production image or changing
`crates/wt-integration-tests/fixture-images.txt`. It does not affect normal
worlds.

## Clear installed development state

To remove all WT development state before reinstalling:

```text
make clear
```

The command refuses to run while any `wt-*` libvirt domain exists. After that
check, it removes the current user's registry and managed SSH inventory plus the
installed server config, golden and test images, manifests, and world files. It
does not uninstall packages or binaries. `make clear` delegates to
`scripts/clear-server`.

## Tests

```text
cargo build --workspace
cargo test --workspace
```

All tests run, including the real KVM acceptance test. See [TESTS.md](./TESTS.md).
