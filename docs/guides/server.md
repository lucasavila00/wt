# Server operations

WT servers run Ubuntu 24.04 amd64 with KVM. Install as the normal `wt` user,
never as root.

Copy and edit the install input:

```text
cp examples/server-config/wt-server.development.toml ./server.toml
scripts/install-server --config ./server.toml
```

The installer prepares libvirt, the shared capacity registry, registry cache,
agent Git gateway, and two retained-world images:

- a devcontainer image with Docker, Git, the Dev Container CLI, and guest tools;
- a host image with upstream Ubuntu, OpenSSH, QEMU guest support, Byobu, and
  tmux.

The two image paths must be different files in the same directory. Runtime
configuration is written to `/etc/wt/server.toml`. Shared CPU, RAM, and disk
limits come from `[capacity]` in the install input and are materialized at
`/etc/wt/capacity.toml`.

A world disk cannot be smaller than its image's `build_disk_gib`. The client
defaults to 32 GiB; a larger build image requires a larger world request.

The current server install requires at least one agent Git provider. Its token,
SSH private key, and trusted host keys stay in encrypted systemd credentials.
Host recipes never receive them. Tests use local fake provider services and
keys, not developer credentials.

## Reset

This world-kind schema has no migration and keeps protocol version 1. Before
installing it over an older WT version, run from the repository root:

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
configuration drift diagnostic.
