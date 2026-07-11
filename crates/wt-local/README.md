# wt-local

Single-site helper: **control-plane API + registry + embedded backend**.

Era 1 runs on the same Ubuntu workstation as the CLI. `wt` invokes it directly as a helper command: one JSON request in, one JSON response out. Owner = local OS user.

## Role

| Does | Does not |
|------|----------|
| Expose control-plane ops as a CLI-spawned helper (stdio JSON) | Require separate bearer-token product for bare metal |
| Invoke `wt-libvirt` for worlds | Implement libvirt/KVM lifecycle itself |
| Local inventory + domain reconcile | |

Design: [docs/arch/control-plane.md](../../docs/arch/control-plane.md), [docs/arch/cli.md](../../docs/arch/cli.md), [docs/arch/bare-metal-agent.md](../../docs/arch/bare-metal-agent.md).

## Run

```text
cargo run -p wt-local
```

## Install on Ubuntu

Target: Ubuntu 24.04 amd64 with hardware virtualization enabled.

From a `wt` source checkout:

```text
scripts/install-site
```

Run it as the site user, not with `sudo`. The script uses `sudo` for host changes.

Prerequisite: stable Rust toolchain from rustup.

The script:

- Checks Ubuntu 24.04 amd64.
- Installs build, libvirt/KVM, and image tools.
- Requires working `/dev/kvm`.
- Adds the site user to `libvirt` and `kvm`.
- Starts and enables the default libvirt network.
- Downloads and prepares the Docker-ready Ubuntu image once.
- Creates `/var/lib/wt/images` and `/var/lib/libvirt/images/wt`.
- Builds and installs `/usr/local/bin/wt-local`.

Log out and back in if the script changes group membership.

The `libvirt` group controls the host hypervisor. Only grant it to trusted site users.

Each invoking user keeps its registry at `~/.local/state/wt/instances.db`. Worlds share `/var/lib/libvirt/images/wt` and the site libvirt daemon.

### Smoke test

```text
printf '%s\n' '{"protocol_version":1,"operation":"list"}' | wt-local api
```

The command writes one JSON response to stdout.

### Runtime overrides

| Variable | Default |
|----------|---------|
| `WT_STATE_DIR` | `~/.local/state/wt` |
| `WT_IMAGE` | `/var/lib/wt/images/wt-ubuntu-24.04-amd64.qcow2` |
| `WT_WORLDS_DIR` | `/var/lib/libvirt/images/wt` |
| `WT_LIBVIRT_URI` | `qemu:///system` |
| `WT_LIBVIRT_NETWORK` | `default` |
| `WT_GUEST_MEMORY_MIB` | `2048` |
| `WT_GUEST_VCPUS` | `2` |
| `WT_GUEST_DISK_GIB` | `16` |
| `WT_GUEST_BOOT_TIMEOUT_SECONDS` | `300` |

`qemu:///system` is libvirt's system-driver URI for QEMU/KVM domains. `wt-libvirt` defines KVM domains only.

## Status

Era 1 implementation in progress.
