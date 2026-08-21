# Server operations

WT servers run Ubuntu 24.04 amd64 with KVM. Install as the normal `wt` user,
never as root. Log in to Codex as that user before installation; the installer
requires a regular, user-owned `/home/wt/.codex/auth.json`.

```text
cp examples/server-config/wt-server.development.toml ./server.toml
scripts/install-server --config ./server.toml
```

The installer prepares libvirt, capacity state, the agent tool gateway, and a
verified retained-world golden image. The final image contains Git, OpenSSH,
QEMU guest support, Byobu, tmux, Codex, and WT's host helpers.

Runtime configuration is written to `/etc/wt/server.toml`. CPU, RAM, and disk
limits are materialized at `/etc/wt/capacity.toml`.

The installer creates the server-backed Codex sessions directory and a
read-only authentication export. Running worlds receive both through virtiofs.
Shared sessions are outside world disk quotas and need a separate backup. Do
not open one conversation in two worlds simultaneously.

Refresh expired Codex authentication as the server `wt` user. A systemd path
unit atomically republishes `auth.json`; running worlds receive the replacement
automatically and cannot write it back.

Provisioning is not resumable. Remove a failed world and recreate it from the
golden image. A world disk cannot be smaller than the image build disk.

Provider tokens, SSH private keys, and trusted host keys remain in encrypted
systemd credentials. Worlds receive scoped grants, never provider credentials.

## Reset

`make clear` removes runtime state while preserving verified golden images.
`make nuke` removes the complete WT installation state, including worlds,
images, services, generated configuration, grants, registry, and encrypted
credentials. Neither command removes source credentials or installed host
packages and binaries.

Golden-image rebuilds affect only new worlds. Existing disks are independent
and retain their current contents until recreated.
