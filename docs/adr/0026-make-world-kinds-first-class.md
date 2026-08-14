# ADR 0026: Make world kinds first-class

- Status: Accepted
- Date: 2026-08-14
- Amends: [ADR 0016](0016-keep-qemu-and-remove-redundant-world-boot-work.md),
  [ADR 0023](0023-run-github-actions-jobs-in-ephemeral-kvm-guests.md), and
  [ADR 0024](0024-use-a-shared-guest-registry.md)

## Decision

WT supports three world kinds:

| Kind | Application | Lifetime |
|------|-------------|----------|
| `devcontainer` | Repository devcontainer | Named and retained |
| `host` | Ubuntu guest | Named and retained |
| `github-ci` | GitHub Actions runner | Single-use |

Kind is immutable and present in registry records, API values, and inventory.
Creation requests are tagged by kind.

The `devcontainer` contract remains unchanged.

A `host` world exposes the Ubuntu guest directly over SSH. Creation requires
cloud-init user-data. WT passes it through unchanged and keeps machine identity,
network, login, and SSH configuration in separate NoCloud data. Readiness
requires successful cloud-init completion. The exact user-data is part of the
hashed create fingerprint and is not stored.

A `host` world boots from a dedicated minimal Ubuntu image. User-data runs as
root and may break WT SSH access. Creation proves direct login with a one-use WT
key, removes that key, and fails unless SSH is ready.

WT adds no checkout, agent Git grant, devcontainer, app SSH, or editor
integration to a host world. The recipe receives no implicit WT credentials.

A `github-ci` world uses the runner lifecycle from ADR 0023. `wt-runner`
creates it for one job and destroys it afterward. It cannot be restarted,
forked, reused, or accessed interactively.

## Ownership

All kinds share KVM, capacity admission, disk ownership, registry identity,
reconciliation, and cleanup. Application state and lifecycle remain typed by
kind.

`wt-server` owns `devcontainer` and `host` worlds. `wt-runner` owns
`github-ci` worlds. `wt new host` accepts cloud-init user-data instead of Git
and devcontainer inputs.

WT does not place developer private keys, provider tokens, or GitHub App
credentials in worlds.

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
- The `wt-runner` crate becomes `wt-github-ci`; its service binary remains
  `wt-runner`.

Executable and service names remain unchanged.

Assets live under `assets/world/shared`, `assets/world/devcontainer`,
`assets/world/host`, or `assets/world/github-ci`. Physical server assets live
under `assets/server`; they do not use the host-world namespace.

Mixed crates use `shared`, `devcontainer`, `host`, and `github_ci` module
names. Kind-specific examples live under `examples/world/KIND`; KVM tests live
under `tests/kvm/KIND`. Flat or generic names must not contain kind-specific
behavior.

## Reset and protocol

Installing this change requires `make nuke`. It destroys all WT guests and
disks and deletes the SQLite registry and installed configuration. There is no
migration or preserved state.

The protocol remains version 1. Request, response, and registry definitions are
replaced in place.
