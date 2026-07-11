# Tests

All tests always run. No KVM skip path.

## System requirements

Install the local site first:

```text
scripts/install-site --config config/wt-local.development.toml
```

The KVM E2E test requires:

| Resource | Expected on this workstation |
|----------|------------------------------|
| KVM | `/dev/kvm`, readable and writable by the test user |
| Groups | Active `libvirt` and `kvm` membership |
| Site config | `/etc/wt/local.toml`, `root:root`, mode `0644` |
| Golden image | `/var/lib/wt/images/wt-ubuntu-24.04-amd64.qcow2`, `libvirt-qemu:kvm`, mode `0644` |
| Image manifest | `/var/lib/wt/images/wt-ubuntu-24.04-amd64.qcow2.manifest.json`, `root:root`, mode `0644` |
| Libvirt network | `default`, active, persistent, autostart, DHCP enabled |
| World directory | `/var/lib/libvirt/images/wt`, site user:`kvm`, mode `2770`, writable |
| SSH Git fixture | Host `openssh-server`; installed by `scripts/install-site` |
| Sample repository | `/home/lucas/fluff/jsdev`, or host network access for the fallback clone |

These paths come from `/etc/wt/local.toml`. `wt-setup` creates and verifies them. Installation details: [wt-local](./crates/wt-local/README.md#install-on-ubuntu).

## Run

```text
cargo build --workspace
cargo test --workspace
```

The workspace run includes:

- Unit validation and wire tests.
- Injected worker integration tests.
- Real SSH clone of the jsdev sample -> requested ref -> stock devcontainer -> push -> strict guest SSH -> remove against libvirt/KVM.

The KVM test uses a temporary user registry. It removes its domain and world directory before returning.
