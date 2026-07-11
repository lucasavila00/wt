# Tests

All tests always run. No KVM skip path.

## System requirements

Install the local site first:

```text
scripts/install-site --config config/wt-local.development.toml
scripts/prepare-test-image --config config/wt-local.development.toml
```

Run both commands in an interactive terminal because they invoke `sudo`. The
second command is a one-time cache warm-up. It builds a separate qcow2 backing
image containing the container images used by the KVM fixture. It does not
modify the production golden image.

Re-run `scripts/prepare-test-image` after:

- Rebuilding or replacing the production golden image.
- Changing `crates/wt-integration-tests/fixture-images.txt`.
- Updating the jsdev fixture to reference different Compose images.

The test deliberately fails with a preparation command when the cache is
missing or stale; it never silently falls back to repeated registry downloads.

The KVM E2E test requires:

| Resource | Expected on this workstation |
|----------|------------------------------|
| KVM | `/dev/kvm`, readable and writable by the test user |
| Groups | Active `libvirt` and `kvm` membership |
| Site config | `/etc/wt/local.toml`, `root:root`, mode `0644` |
| Golden image | `/var/lib/wt/images/wt-ubuntu-24.04-amd64.qcow2`, `libvirt-qemu:kvm`, mode `0644` |
| Image manifest | `/var/lib/wt/images/wt-ubuntu-24.04-amd64.qcow2.manifest.json`, `root:root`, mode `0644` |
| Test image cache | `/var/lib/wt/images/wt-ubuntu-24.04-amd64.integration-tests.qcow2` plus its manifest; created by `scripts/prepare-test-image` |
| Libvirt network | `default`, active, persistent, autostart, DHCP enabled |
| World directory | `/var/lib/libvirt/images/wt`, site user:`kvm`, mode `2770`, writable, with ACL `user:libvirt-qemu:--x` |
| SSH Git fixture | Host `openssh-server`; installed by `scripts/install-site` |
| Git server | SSH access to `git@github.com:lucasavila00/jsdev-sample.git` |

These paths come from `/etc/wt/local.toml`. `wt-local-setup` creates and verifies them. Installation details: [wt-local](./crates/wt-local/README.md#install-on-ubuntu).

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
Its disposable disk is backed by the integration cache, so the full test still
uses a fresh VM while avoiding repeated downloads of the fixture's Node and
Redis images. Run with `--nocapture` to print phase timings:

```text
cargo test -p wt-integration-tests --test kvm_e2e -- --nocapture
```

To verify or explicitly rebuild the cache through the setup CLI:

```text
cargo run --release -p wt-local-setup -- image test-cache build --config config/wt-local.development.toml
cargo run --release -p wt-local-setup -- image test-cache rebuild --config config/wt-local.development.toml
```

`build` reuses matching installed state. `rebuild` replaces the cache and
refuses to run while a `wt-*` domain is active.
