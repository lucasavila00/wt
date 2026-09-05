# Server operations

WT servers run Ubuntu 24.04 amd64 with KVM. Install as the normal `wt` user,
never as root. Log in to Codex as that user before installation; the installer
requires a regular, user-owned `/home/wt/.codex/auth.json`.

```text
cp examples/server-config/wt-server.development.toml ./server.toml
scripts/install-server --config ./server.toml
```

The installer prepares libvirt, capacity state, the agent tool gateway, and a
verified guest golden image. Every image contains Git, OpenSSH, QEMU
guest support, Byobu, tmux, Diffo, WT's guest helpers, current Rust/Cargo,
Go, Python/uv, Node.js/nvm, build tools, CLI utilities including ShellCheck,
Docker with Compose, the interactive Codex CLI, and agapi with its matching
private Codex binary.

Golden-image rebuilds reuse a verified local development-tools cache and refresh
WT's guest binaries instead of reinstalling language toolchains. Changing the
tool-layer policy invalidates the cache and fetches current upstream releases.

## Codex defaults

WT supplies `gpt-5.6-terra` and `high` reasoning effort as the defaults for new
Codex threads. They are defaults, not restrictions: an explicit model or
reasoning selection for a thread overrides them. Existing threads keep their
current selection.

WT bakes these settings into each golden image at
`/etc/codex/requirements.toml`; their repository source is
[assets/world/guest/codex-requirements.toml](../../assets/world/guest/codex-requirements.toml).
Change that source and rebuild the image instead of editing a world. Existing
world overlays retain their current backing image, so they do not receive a
changed default.

Every new world has `codex` on PATH, with the version pinned in
`assets/world/guest/codex-version`. The image supplies the initial user configuration
for unrestricted execution inside the guest and the WT world-context hook. Users
can edit that configuration in their world. Rebuild the image to change the
preinstalled CLI; existing worlds retain their installed version.

New images include the standalone [agapi runtime](../../crates/products/agapi/README.md),
built from the image's checkout, and its matching Codex binary under
`~/.local/share/agapi/codex`, separately from the interactive CLI. Start it when
needed, choosing a workspace and state directory:

```sh
agapi --state-dir "$HOME/.local/state/agapi/my-project" serve \
  --workspace /absolute/workspace \
  --codex "$HOME/.local/share/agapi/codex/bin/codex"
```

Supervise `agapi serve` independently of WT. It does not start automatically.
Use `scripts/install-agapi VERSION` to update the pair inside an existing world;
the user-local command takes precedence over the image's original binary.

Codex authentication is shared read-only. Each world receives a read-write
mount of only its own server-backed sessions directory; user configuration,
databases, indexes, logs, and locks remain local to that world.

Installation requires a clean checkout: staged, unstaged, and untracked files
are all rejected before a production build starts. `wts --version`
prints the package version and full source commit SHA. The same identity is
recorded in guest-image provenance and logged when the server starts.

Runtime configuration is written to `/etc/wt/server.toml`. CPU, RAM, and disk
limits are materialized at `/etc/wt/capacity.toml`.

Request-initialization failures are recorded in the server journal. Inspect
them with `journalctl -u wts.service` when a client reports a context
refresh failure.

The installer creates a server-backed Codex sessions root and a read-only
authentication export. Each world receives its own sessions directory through
virtiofs. History is outside world disk quotas and needs a separate backup; it
is not live pane state.

Refresh expired Codex authentication as the server `wt` user. A systemd path
unit atomically republishes `auth.json`; running worlds receive the replacement
automatically and cannot write it back.

The server also exports `/home/wt/.ssh/authorized_keys` read-only to every
world. A systemd path unit republishes changes atomically, so adding or removing
a key on the KVM host updates SSH access to all running worlds without
recreating them. Other files under `/home/wt/.ssh` are never exposed.

Provisioning is not resumable. Remove a failed world and recreate it from the
golden image. A world disk cannot be smaller than the image build disk.

Provider tokens and SSH private keys remain in encrypted systemd credentials.
The gateway identifies each guest from its vsock peer CID and active libvirt
domain; worlds receive no gateway or provider credentials. Gateway Git does
not pin provider SSH host keys, so provider key rotation cannot block worlds.

## Reset

`make clear` removes runtime state while preserving verified golden images.
`make nuke` removes the complete WT installation state, including worlds,
images, services, generated configuration, registry, and encrypted
credentials. Neither command removes source credentials or installed host
packages and binaries.

Publishing a rebuilt golden image does not rewrite existing world overlays.
Each overlay still names its content-addressed backing-image generation, which
must remain present and intact for that world to continue working.
