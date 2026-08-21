# Host worlds

A host world is a retained Ubuntu guest configured by cloud-init. It has no
implicit checkout, Docker setup, devcontainer, or app SSH server. A recipe may
create its own checkout, as the project example does.

Put a non-empty cloud-init user-data file at
`~/.config/wt/cloud-init.yaml`, then create a named host:

```text
wt new host NAME
```

WT honors `XDG_CONFIG_HOME` when it is set. For a one-off recipe, override the
default with `wt new host NAME --user-data ./host.yaml`. The client installer
creates a thin recipe with Diffo when the default file is missing and does not
overwrite an existing file. It does not install Rust/Cargo or clone a project.

WT first boots the retained host image, which already owns the `wt` login at
UID/GID `1001:1001`. Provisioning validates that image contract, stages the
selected file root-only, transfers the workstation's global Git author, and
verifies SSH. `wt new host` then opens Byobu.
Cloud-init starts there and runs the standard init, config, and final stages.
Output stays in the pane and `/var/log/cloud-init-output.log`.

The recipe is included in a hashed create fingerprint but is not stored in
SQLite. It and its output remain on the guest disk, so neither is a secret
store. WT rejects top-level host-key, cloud-init stage, merge, and output fields
because it owns those parts of setup.

Success changes the world from `setup` to `running`. A setup failure changes it
to `error` and keeps both SSH aliases. Earlier provisioning failures have no SSH
alias. Every failed host remains visible in `wt ls` and removable with `wt rm`.
WT never reruns a failed recipe.

The regular alias attaches to a persistent Byobu session. The `-vs` alias is
the same guest SSH endpoint with no forced command:

```text
ssh CONTEXT.NAME
ssh CONTEXT.NAME-vs
```

`wt ssh NAME` refreshes the managed aliases and connects to the qualified
persistent Byobu alias.

There is no `-host` alias. `wt code` rejects host worlds; use `-vs` directly for
plain SSH, SFTP, or an editor.

WT does not automatically forward the workstation's SSH agent. For an explicit
direct session, `ssh -A CONTEXT.NAME-vs` uses OpenSSH's native forwarding. This
exposes the agent without the gateway's restrictions and is the developer's
responsibility. With `ssh -A CONTEXT.NAME`, the forwarded socket belongs to the
current SSH connection while Byobu and its processes persist; existing panes
may retain a stale socket after disconnect or reattach. WT does not retarget
that socket, and host setup never receives it.

The retained image contains Codex. Provisioning installs and activates
`wt-codex-integration`, mounts the server-backed sessions read-write at
`/home/wt/.codex/sessions`, and links `.codex/auth.json` to the server login
exposed read-only. These mounts are restored and verified whenever a stopped
host world starts.

Every host receives `ag-git` and a revocable gateway grant. Configured provider
URLs use the gateway automatically. The grant can read every available
repository and write only branches under `wt/`; provider credentials remain on
the server. Explicit OpenSSH agent forwarding is a separate access path and is
not restricted by the gateway.

The host image is separate from the devcontainer image. It adds OpenSSH, QEMU
guest support, the pinned Byobu package, compiled tmux, Ghostty terminfo, and
the shared WT terminal profile, including the fixed `wt` image user and its
`/home/wt/.byobu` files. Ubuntu's Git remains available, and WT adds no
implicit checkout or provider credentials.

Golden-image replacement does not migrate existing host worlds. Existing disks
retain their current guest user and terminal state; recreate a host world to
use a newly built image foundation.
