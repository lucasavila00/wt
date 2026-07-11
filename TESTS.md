# Tests

All tests always run. No KVM skip path.

## Prerequisites

- Ubuntu 24.04 amd64.
- Active `kvm` and `libvirt` groups.
- Working `/dev/kvm` and system libvirt.
- Complete site install from [wt-local](./crates/wt-local/README.md#install-on-ubuntu).
- `/etc/wt/local.toml`, golden image, manifest, network, and world directory ready.

## Run

```text
cargo build --workspace
cargo test --workspace
```

The workspace run includes:

- Unit validation and wire tests.
- Injected worker integration tests.
- Real `wt new <name>` -> `wt ls` -> `wt rm <name>` against libvirt/KVM.

The KVM test uses a temporary user registry. It uses the installed site config and golden image.
