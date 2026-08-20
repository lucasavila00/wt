# ADR 0026: Make world kinds first-class

- Status: Accepted
- Date: 2026-08-14
- Amends: [ADR 0016](0016-keep-qemu-and-remove-redundant-world-boot-work.md),
  [ADR 0023](0023-run-github-actions-jobs-in-ephemeral-kvm-guests.md), and
  [ADR 0024](0024-use-a-shared-guest-registry.md)
- Amended by: [ADR 0040](0040-stop-automatic-ssh-agent-forwarding.md) and
  [ADR 0043](0043-own-retained-guest-foundation-in-shared-images.md)

## Decision

WT supports three world kinds:

| Kind | Application | Lifetime |
|------|-------------|----------|
| `devcontainer` | Repository devcontainer | Named and retained |
| `host` | Ubuntu guest | Named and retained |
| `github-ci` | GitHub Actions runner | Single-use |

Kind is immutable and present in registry records, API values, and inventory.
Creation requests are tagged by kind.

The devcontainer application contract remains unchanged; its retained guest
foundation is shared with the host world as described by ADR 0043.

A `host` world boots the retained Ubuntu image with the shared `wt` login,
OpenSSH, QEMU guest support, and Byobu. The image owns `wt` at UID/GID
`1001:1001`; host provisioning validates that contract, stages the submitted
cloud-init YAML, verifies SSH, and returns the world in `setup`. The boot seed
contains only the data needed to bring up the machine and network. The image
defers cloud-init's normal init
modules until setup; WT does not delete cloud-init state to run them again. WT
leaves SSH disabled in the reusable image. After the boot-time cloud-init
service finishes, WT generates the world's SSH host keys and enables SSH. Those
keys remain pinned through setup. WT flushes the staged setup state before
returning the world, so an immediate hard stop does not lose it. Host recipes
cannot override WT-owned host identity, cloud-init stage lists, merge behavior,
or output.

`wt new host` opens the regular SSH alias. It forwards the workstation SSH
agent and attaches to a persistent Byobu session. The first session runs
cloud-init's standard init, config, and final modules with the submitted YAML.
Cloud-init uses a stable agent socket so reconnecting does not leave existing
panes pointing at a deleted socket. The latest Byobu connection supplies the
agent.

The setup pane follows `/var/log/cloud-init-output.log` while the system service
runs. A completion marker promotes the world to `running`. A failure marker
moves it to `error`. A persistent started marker prevents retries. While the
setup service is active, the world remains in `setup`; a started world with no
active service or final marker becomes `error`. WT keeps every failed host for
inspection and removal. A failed host keeps both
SSH aliases during deferred setup; failures before the world reaches `setup`
have no aliases.

The `-vs` alias is direct guest SSH with agent forwarding and no forced command.
It does not start setup.

The submitted YAML runs as root, is stored root-only in the guest, and is part
of the hashed create fingerprint. It is not stored in SQLite. If the Byobu
connection closes, cloud-init keeps running, but commands using the forwarded
agent may fail. Reconnecting refreshes the socket; it does not retry commands.

WT adds no checkout, agent Git grant, devcontainer, or app SSH to a host world.
Ubuntu's Git remains available. WT never copies private keys into the guest.

World names cannot end in `-host` or `-vs`; those suffixes are reserved for SSH
aliases.

A `github-ci` world uses the single-job lifecycle in `wt-github-ci`. It cannot
be restarted, forked, reused, or accessed interactively. The operator service
and image installer remain follow-up work under ADR 0023.

## Ownership

All kinds share KVM, capacity admission, disk ownership, and registry identity.
Reconciliation and cleanup remain typed by kind and owned by their lifecycle.

`wt-server` owns `devcontainer` and `host` worlds. The future runner service
will own `github-ci` worlds. `wt new host` accepts cloud-init user-data instead
of Git and devcontainer inputs.

WT does not place developer private keys, provider tokens, or GitHub App
credentials in worlds.

WT rejects inputs that conflict with kind-owned state. It does not add fallback
or compatibility behavior for unsupported recipes.

Host creation fails before provisioning when user-data sets top-level
`ssh_keys`, `ssh_deletekeys`, `cloud_init_modules`, `cloud_config_modules`,
`cloud_final_modules`, `merge_how`, `merge_type`, or `output`.

## Code layout

Generic names are reserved for code used by more than one world kind.
Kind-specific crate names use `wt-KIND`; they do not repeat `world`.

- `wt-api`, `wt-cli`, `wt-command`, `wt-integration-tests`, `wt-libvirt`,
  `wt-provider`, `wt-registry`, `wt-server`, and `wt-server-setup` remain shared
  crates.
- Devcontainer provisioning moves from `wt-provider` to `wt-devcontainer`.
- `wt-guest` becomes `wt-devcontainer-guest`.
- `wt-agent-git` becomes `wt-devcontainer-git`.
- Host provisioning lives in `wt-host`.
- The runner lifecycle crate is `wt-github-ci`; no runner executable is shipped
  yet.

Executable and service names remain unchanged.

Assets live under `assets/world/shared`, `assets/world/devcontainer`,
`assets/world/host`, or `assets/world/github-ci`. Physical server assets live
under `assets/server`; they do not use the host-world namespace.

## Reset and protocol

Installing this change requires `make nuke`. On the standard installation it
destroys all WT guests and disks and deletes the SQLite registry and installed
configuration. Source credentials and installed binaries remain. There is no
migration or preserved runtime state.

The protocol and schema remain version 1. Request, response, and registry
definitions are replaced in place. No compatibility code or migration is kept.
