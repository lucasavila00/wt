# Tests

All tests always run. No KVM skip path.

## Setup

Target: Ubuntu 24.04 amd64.

Install the site first:

```text
scripts/install-site --config config/wt-local.development.toml
```

The command is idempotent for matching state. It installs and verifies every resource below.

## Host

The test user must have active `libvirt` and `kvm` groups. `/dev/kvm` must be readable and writable.

```text
id
test -r /dev/kvm -a -w /dev/kvm
```

Example:

```text
uid=1000(lucas) gid=1000(lucas) groups=...,126(libvirt),993(kvm)
```

## Site config

`/etc/wt/local.toml` is the exact config used by `wt-local` and the KVM E2E test. No runtime override exists.

Expected:

```text
root:root 644 /etc/wt/local.toml
```

This workstation installs the development sample verbatim:

```text
cmp config/wt-local.development.toml /etc/wt/local.toml
stat -c '%U:%G %a %n' /etc/wt/local.toml
cargo run -p wt-setup -- validate --config /etc/wt/local.toml
```

## Golden image

`image.installed_path` points to the qcow2 backing image used by every world overlay. It contains Ubuntu 24.04, Docker Engine, Docker Compose v2, and QEMU guest agent.

Current path and expected file state:

```text
libvirt-qemu:kvm 644 /var/lib/wt/images/wt-ubuntu-24.04-amd64.qcow2
```

Verify:

```text
stat -c '%U:%G %a %n' /var/lib/wt/images/wt-ubuntu-24.04-amd64.qcow2
qemu-img check /var/lib/wt/images/wt-ubuntu-24.04-amd64.qcow2
```

## Image manifest

The manifest sits next to the golden image. It binds the installed image to:

- Pinned Ubuntu source SHA-256.
- Exact site config SHA-256.
- Final golden image SHA-256.
- Installed guest package versions.

Expected file state:

```text
root:root 644 /var/lib/wt/images/wt-ubuntu-24.04-amd64.qcow2.manifest.json
```

Current example:

```json
{
  "version": 1,
  "source_sha256": "5fa5b05e5ec239858c4531485d6023b0896448c2df7c63b34f8dae6ea6051a44",
  "config_sha256": "d8865ee819487a30677435b995ee728d8e0904ad2db3cbb453a1a9c41d49d1a9",
  "golden_sha256": "d6364502e87af5ba27e765d3a4749f81573a1d973d90ee8aebc6943675d12062",
  "packages": [
    "docker-compose-v2=2.40.3+ds1-0ubuntu1~24.04.1",
    "docker.io=29.1.3-0ubuntu3~24.04.2",
    "qemu-guest-agent=1:8.2.2+ds-0ubuntu1.17"
  ]
}
```

Verify the current golden hash:

```text
sha256sum /var/lib/wt/images/wt-ubuntu-24.04-amd64.qcow2
```

Expected:

```text
d6364502e87af5ba27e765d3a4749f81573a1d973d90ee8aebc6943675d12062  /var/lib/wt/images/wt-ubuntu-24.04-amd64.qcow2
```

## Libvirt network

`libvirt.network` names an existing system libvirt network. It must be active, persistent, and enabled at boot. DHCP must be available for guest IP discovery.

This workstation uses `default`:

```text
virsh -c qemu:///system net-info default
```

Expected fields:

```text
Name:           default
Active:         yes
Persistent:     yes
Autostart:      yes
Bridge:         virbr0
```

Old DHCP leases are allowed.

## World directory

`libvirt.worlds_dir` stores temporary per-world overlays, cloud-init seeds, and metadata.

Expected:

```text
lucas:kvm 2770 /var/lib/libvirt/images/wt
```

Verify:

```text
stat -c '%U:%G %a %n' /var/lib/libvirt/images/wt
test -w /var/lib/libvirt/images/wt
```

The directory should contain no `wt-image-build` state before tests. The E2E test creates one uniquely named world and removes its domain and directory before returning.

```text
test ! -e /var/lib/libvirt/images/wt/wt-image-build
! virsh -c qemu:///system dominfo wt-image-build
```

## Run

```text
cargo build --workspace
cargo test --workspace
```

The workspace run includes unit validation, injected worker integration, and real `wt new <name>` -> `wt ls` -> `wt rm <name>` against libvirt/KVM.

The KVM test uses a temporary user registry. Site config, golden image, network, and world directory are real.
