# Plan: share agent conversations between worlds

## Goal

Keep Codex conversations after a world is deleted, and make them available in
every retained world on the same WT server.

WT will share folders between the server and the world VM. It will not change
devcontainer recipes. A repository that wants the folders inside its container
must add its own bind mounts.

## Configuration

Add an optional top-level list to the server configuration:

```toml
[[shared_folders]]
source = "/home/wt/.codex/sessions"
target = ".codex/sessions"
```

`source` is an absolute directory on the WT server. `target` is relative to
the `wt` user's home in each VM. An empty list keeps today's behavior.

Validate that sources are normalized absolute paths and targets are normalized
relative paths without `..`. Reject duplicate sources and targets. Shared
folders must not overlap WT images, world disks, the registry cache, or the
installed binaries.

Put the entry in the development and KVM example configurations. The server
installer creates missing source directories. Runtime startup fails clearly if
a configured source is missing or is not a directory.

## VM attachment

Give every configured folder a stable virtiofs tag based on its list position,
such as `wt-shared-0`.

Extend the libvirt machine configuration with the source directory and tag.
When at least one share exists, generated domain XML will include shared memory
support and one virtiofs filesystem device per folder. Escape source paths in
the same way as disk and network values.

The same machine configuration is used for host and devcontainer worlds, so
both kinds receive the devices. GitHub CI
machines do not use the retained-world server configuration and receive none.

## Mounting inside the VM

Add one small guest-side mount procedure shared by host and devcontainer-world
images. The server passes it the virtiofs tag and target path.

For each folder it will:

1. Create `/home/wt/<target>`.
2. Add a persistent virtiofs mount to `/etc/fstab`.
3. Mount it immediately and fail setup if mounting fails.
4. Make the mounted directory private and writable by the `wt` user.
5. Verify the expected virtiofs tag is mounted at the expected path.

Run this after the `wt` account exists and before user recipes start. Verify
the mounts again when a stopped world starts. World deletion removes only the
VM; it never removes the source directories on the WT server.

Both golden images must assign the same numeric user and group IDs to `wt`,
because virtiofs preserves ownership. Add an image test for that assumption.
If the IDs differ today, fix image creation so they are stable before enabling
the default shares.

## Devcontainers

Do not add mounts to `devcontainer up` or inspect container users.

The shared folders are available on the VM at:

```text
/home/wt/.codex/sessions
```

A repository owner using Docker Compose can expose them to a container with
service bind mounts, choosing targets that match its `remoteUser`:

```yaml
services:
  app:
    volumes:
      - /home/wt/.codex/sessions:/home/vscode/.codex/sessions
```

The repository owns container-user permissions. WT only guarantees the VM
paths and does not assume that every devcontainer uses `/home/vscode`.

## Code changes

- `wt-server`: parse and validate `shared_folders`; pass sources to libvirt and
  tag/target pairs to both retained-world provisioners.
- `wt-server-setup`: carry the entries from install input to runtime config,
  create source directories, and update the rendered-config snapshot.
- `wt-libvirt`: render shared memory and virtiofs devices in domain XML.
- `wt-host` and `wt-devcontainer`: install, mount, and verify the VM paths using
  the same guest-side procedure.
- `assets/world/shared`: hold the mount procedure used by both world images.
- `examples/server-config`: enable the Codex folder by default.
- World and server docs: explain the VM paths and the user-owned Docker Compose
  bind mounts.

No container discovery or container launch code needs to change.

## Tests

Add focused tests for:

- valid, empty, duplicate, overlapping, absolute-source, and relative-target
  server configuration;
- install-input materialization and all example configurations;
- complete libvirt domain XML with zero, one, and two shared folders;
- escaping source paths and stable virtiofs tags;
- guest mount configuration and useful failures;
- host and devcontainer-world provisioning calls;
- stable `wt` user and group IDs in both images.

Run formatting, tests, and Clippy for every affected Rust crate. Run `bash -n`,
ShellCheck, and behavior tests for the mount script.

Add a real KVM test that creates a host world and a devcontainer world, writes
a marker through one VM, reads it through the other, restarts both worlds, and
checks the marker again. Delete a world and confirm that the server source still
contains the marker. The devcontainer fixture may declare the two Docker
Compose bind mounts to prove the documented user-controlled path, but
production code must not inject them.

## Limits

Every retained world can read and change these folders. They are outside world
disk quotas and snapshots and need a separate backup.

WT will not prevent two worlds from opening the same conversation at once.
Users must avoid doing that because both agents may write the same file.

This change does not migrate existing conversation files or existing world
definitions. New installs get the example defaults; existing servers opt in by
adding the entries and recreating their retained worlds.

## Done when

- New host and devcontainer worlds see the same configured VM folders.
- Conversation files survive world deletion and server restart.
- A repository can bind the VM folders into its devcontainer without WT help.
- Unconfigured servers behave exactly as they do today.
- Unit, shell, and real KVM checks pass.
