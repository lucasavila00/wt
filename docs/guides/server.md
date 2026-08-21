# Server operations

WT servers run Ubuntu 24.04 amd64 with KVM. Install as the normal `wt` user,
never as root.

Log in to Codex as that user before installing WT. Installation requires
`/home/wt/.codex/auth.json` to be a regular, non-symlink file owned by `wt`:

```text
codex
```

Copy and edit the install input:

```text
cp examples/server-config/wt-server.development.toml ./server.toml
scripts/install-server --config ./server.toml
```

The installer prepares libvirt, the shared capacity registry, registry cache,
agent tool gateway, and two retained-world images:

- a devcontainer image with Docker, Git, the Dev Container CLI, and guest tools;
- a host image with upstream Ubuntu, OpenSSH, QEMU guest support, Byobu, and
  tmux.

The two image paths must be different files in the same directory. Runtime
configuration is written to `/etc/wt/server.toml`. Shared CPU, RAM, and disk
limits come from `[capacity]` in the install input and are materialized at
`/etc/wt/capacity.toml`.

Codex is a required retained-world integration and has no server setting. The
installer creates `/home/wt/.codex/sessions` and a WT-managed auth export. Every
retained host and devcontainer world receives the sessions directory read-write
and the server login read-only. The shared sessions survive world deletion and
server restart, but are outside world disk quotas and snapshots and need a
separate backup. Do not open the same conversation in two worlds at once.

Both retained images install Codex. Provisioning installs and activates
`wt-codex-integration`, which reconciles shared conversations before starting the real
Codex CLI. Devcontainer worlds inject both executables and the fixed Codex
mounts into the primary container automatically. GitHub CI runners receive no
server Codex data.

If Codex authentication expires, refresh it as the server `wt` user. A systemd
path unit republishes an atomically replaced `auth.json` to running worlds;
worlds cannot write the credential back.

A world disk cannot be smaller than its image's `build_disk_gib`. The client
defaults to 32 GiB; a larger build image requires a larger world request.

The current server install requires at least one agent tool provider. Its token,
SSH private key, and trusted host keys stay in encrypted systemd credentials.
Host recipes never receive them. Tests use local fake provider services and
keys, not developer credentials.

All installed WT executables are static musl binaries except `wt-server`.
The server is native to Ubuntu 24.04 because it links `libvirt.so`; setup
installs and validates that host dependency. Installation rejects a designated
static artifact if it contains a dynamic interpreter or GLIBC requirement.

## Reset

To remove all WT runtime state, run from the repository root:

```text
make nuke
```

On the standard installation this stops WT services, destroys every `wt-*` KVM
domain, and removes installed configuration, images, worlds, grants, the SQLite
registry, and the server user's generated inventory. No standard WT runtime
state is preserved. Source credentials and installed packages/binaries remain.

Run `wt sync` on each workstation after the server is installed again. If the
server is intentionally left empty, remove that workstation's stale
`~/.ssh/wt` inventory manually.

Use `make clear` for the smaller runtime reset described by the installed
configuration drift diagnostic. It preserves verified golden images and their
provenance manifests so reinstalling unchanged image inputs does not rebuild
them.

Golden-image rebuilds do not migrate retained worlds. Existing world disks are
independent of their golden image and keep their current guest user, terminal
configuration, and runtime state. Recreate affected worlds after adopting a
changed shared image foundation; use `make nuke` when the complete installation
reset described above is required.
