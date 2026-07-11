# wt-local

Single-site helper: **control-plane API + registry + embedded backend**.

Era 1 runs on the same Ubuntu workstation as the CLI. `wt` invokes `wt-local api` directly. One JSON request in. One JSON response out. Owner = local OS user.

## Role

| Does | Does not |
|------|----------|
| Expose control-plane ops over stdio JSON | Listen on a socket |
| Keep the local instance registry | Use SSH |
| Invoke `wt-libvirt` | Implement libvirt/KVM lifecycle |

Design: [docs/arch/control-plane.md](../../docs/arch/control-plane.md), [docs/arch/cli.md](../../docs/arch/cli.md), [docs/arch/bare-metal-agent.md](../../docs/arch/bare-metal-agent.md).

## Install on Ubuntu

Target: Ubuntu 24.04 amd64. KVM required. Source checkout required.

Install stable Rust with rustup. Clone `wt`. Create a complete site config:

```toml
version = 1

[image]
source_url = "https://cloud-images.ubuntu.com/releases/noble/release-20260615/ubuntu-24.04-server-cloudimg-amd64.img"
source_sha256 = "5fa5b05e5ec239858c4531485d6023b0896448c2df7c63b34f8dae6ea6051a44"
installed_path = "/var/lib/wt/images/wt-ubuntu-24.04-amd64.qcow2"

[libvirt]
network = "default"
worlds_dir = "/var/lib/libvirt/images/wt"

[guest]
memory_mib = 8192
vcpus = 4
disk_gib = 32
boot_timeout_seconds = 300

[install]
binary_dir = "/usr/local/bin"
```

Save it outside `config/`. That directory contains development samples only.

Validate:

```text
cargo run --release -p wt-setup -- validate --config /path/to/site.toml
```

Install:

```text
scripts/install-site --config /path/to/site.toml
```

Run as the site user, not with `sudo`. Run in an interactive terminal. The command invokes `sudo` and may ask for the password.

The installer:

- Installs Ubuntu host packages.
- Adds the site user to `libvirt` and `kvm`.
- Stops when new group membership requires a new login.
- Requires working KVM. No emulation fallback.
- Starts and enables the configured existing libvirt network.
- Creates and verifies configured directories.
- Owns the worlds directory as the site user and `kvm`, mode `2770`.
- Downloads and verifies the pinned Ubuntu source image.
- Builds the Docker/Compose-ready golden image in a temporary KVM guest.
- Installs `wt` and `wt-local` into `install.binary_dir`.
- Copies the supplied config verbatim to `/etc/wt/local.toml`.

Matching state is accepted. Differing config, ownership, modes, partial image state, stale build state, or image provenance fails installation.

`/etc/wt/local.toml` is the only runtime site config. Era 1 has no runtime environment overrides.

Each user registry is fixed at `~/.local/state/wt/instances.db`. Worlds share the configured `libvirt.worlds_dir` and system libvirt daemon.

The `libvirt` group controls the host hypervisor. Only grant it to trusted site users.

## Smoke test

```text
printf '%s\n' '{"protocol_version":1,"operation":"list"}' | wt-local api
```

The command writes one JSON response to stdout.
