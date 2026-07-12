# wt-server

Single-server helper: **control-plane API + registry + embedded backend**.

`wt-server` runs on an Ubuntu/KVM server. `wt` invokes `wt-server api` directly for a
local context or through OpenSSH for a remote context. One JSON request enters
over stdin and one JSON response leaves over stdout. The owner is the OS user
executing the helper.

## Role

| Does | Does not |
|------|----------|
| Expose control-plane ops over stdio JSON | Listen on a socket |
| Keep the server user's instance registry | Implement SSH authentication or policy |
| Invoke `wt-libvirt` | Implement libvirt/KVM lifecycle |

Design: [architecture](../../docs/arch/README.md),
[CLI](../../docs/arch/cli.md), and
[libvirt/KVM backend](../../docs/arch/bare-metal-agent.md).

## Install on Ubuntu

Target: Ubuntu 24.04 amd64. KVM required. Source checkout required.

Install stable Rust with rustup. Clone `wt`. Create a complete install input:

```toml
version = 1

[image]
source_url = "https://cloud-images.ubuntu.com/releases/noble/release-20260615/ubuntu-24.04-server-cloudimg-amd64.img"
source_sha256 = "5fa5b05e5ec239858c4531485d6023b0896448c2df7c63b34f8dae6ea6051a44"
installed_path = "/var/lib/wt/images/wt-ubuntu-24.04-amd64.qcow2"

[libvirt]
network = "default"
worlds_dir = "/var/lib/libvirt/images/wt"

[registry_cache]
state_dir = "/var/lib/wt/registry-cache"
port = 3128
max_size_gib = 64
registries = ["docker.io", "mcr.microsoft.com"]

[git]
identity_file = "~/.ssh/id_ed25519"
known_hosts_file = "~/.ssh/known_hosts"

[guest]
session = "byobu"
memory_mib = 8192
vcpus = 4
disk_gib = 32
boot_timeout_seconds = 300
recipe_timeout_seconds = 900
ssh_authorized_keys_file = "~/.ssh/id_ed25519.pub"

[install]
binary_dir = "/usr/local/bin"
```

Save it outside `config/`.

`guest.session` is the persistent app-session frontend and must be either
`tmux` or `byobu`. The Byobu package uses tmux as its backend.

Validate:

```text
cargo run --release -p wt-server-setup -- validate --config /path/to/install-input.toml
```

Install:

```text
scripts/install-server --config /path/to/install-input.toml
```

Run as the server user, not with `sudo`. Run in an interactive terminal. The command invokes `sudo` and may ask for the password.

The installer:

- Installs Ubuntu host packages.
- Adds the server user to `docker`, `libvirt`, and `kvm`.
- Stops when new group membership requires a new login.
- Requires working KVM. No emulation fallback.
- Starts and enables the configured existing libvirt network.
- Starts and verifies the pinned registry cache and reads its CA for guest setup.
- Creates and verifies configured directories.
- Owns the worlds directory as the server user and `kvm`, mode `2770`, with search-only ACL access for `libvirt-qemu`.
- Downloads and verifies the pinned Ubuntu source image from the install input.
- Builds the Docker/Compose-ready golden image in a temporary KVM guest and
  streams its cloud-init console output with phase timings.
- Installs `wt` and `wt-server` into `install.binary_dir`.
- Materializes the runtime server config at `/etc/wt/server.toml`.

Matching state is accepted. Differing materialized config, ownership, modes,
partial image state, stale build state, or image provenance fails installation.

`/etc/wt/server.toml` is the runtime server config. Install input is the setup
document passed to `--config`. The Git identity must be an encrypted OpenSSH
private key, mode `0600`, owned by the server user. `wt new` prompts on the
client terminal; the passphrase is never stored in server config. The Git,
client-to-server, and guest-login identities have separate roles, but a
deployment may point the first two roles at the same key. There are no runtime
environment overrides.

Each user registry is fixed at `~/.local/state/wt/instances.db`. Worlds share
the configured `libvirt.worlds_dir` and system libvirt daemon.

Provisioning runs in a detached internal `wt-server` process. SQLite stores both
world state and append-only provisioning log chunks; there are no job log files.
Per-world files under `~/.local/state/wt/jobs` exist only for OS locks. Accepted
jobs survive client and OpenSSH disconnects. Worker crashes become `error` on
the next API operation and require explicit `wt rm`.

The `libvirt` group controls the host hypervisor. Only grant it to trusted server users.

## Smoke test

```text
printf '%s\n' '{"protocol_version":1,"operation":"list"}' | wt-server api
```

The command writes one JSON response to stdout.
