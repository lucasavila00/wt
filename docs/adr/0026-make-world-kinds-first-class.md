# ADR 0026: Make world kinds first-class

- Status: Accepted
- Date: 2026-08-14
- Amends: [ADR 0016](0016-keep-qemu-and-remove-redundant-world-boot-work.md),
  [ADR 0023](0023-run-github-actions-jobs-in-ephemeral-kvm-guests.md), and
  [ADR 0024](0024-use-a-shared-guest-registry.md)

## Context

WT started with one kind of world: a named development environment built from
a repository's devcontainer.

We now also run GitHub Actions jobs in short-lived KVM guests, and we want a
simple raw Ubuntu environment. All three use the same KVM, capacity, disk, and
cleanup machinery. They should be different kinds of one thing, not separate
top-level concepts.

## Decision

Make `world` the common application model. Every world has one explicit kind:

| Kind | Meaning | Lifetime |
|------|---------|----------|
| `devcontainer` | The current repository development environment | Named and retained |
| `host` | A raw Ubuntu guest | Named and retained |
| `github-ci` | One GitHub Actions runner | Ephemeral and single-use |

The kind is stored, returned by the API, and shown in inventory. It never
changes in place.

A **devcontainer world** keeps today's behavior: checkout, scoped agent Git
grant, devcontainer, guest and app SSH, persistent session, and editor support.

A **host world** stops at Ubuntu guest readiness. Here `host` means the guest,
not the physical WT server. It has OpenSSH, a managed login account, authorized
public keys, and unique machine and SSH identities. WT gives it no Git input,
checkout, agent Git grant, devcontainer, app SSH, or editor flow. Connecting to
it opens a normal Ubuntu shell.

A host world is not a devcontainer world with empty fields. It gets a small
host-specific provisioner so WT does not create Git or app state accidentally.

Host creation requires a cloud-init user-data document as its recipe; a minimal
`#cloud-config` is valid. WT passes that document through without merging YAML.
WT-owned identity, networking, login, and authorized-key data stay in separate
NoCloud metadata, network configuration, and vendor data. The host becomes
ready only after cloud-init finishes successfully. The exact recipe is part of
the create fingerprint.

This amends ADR 0016 only for host worlds. Other world kinds keep their existing
cloud-init ownership.

The recipe runs as root and may install anything. That does not make installed
software a WT-managed capability or grant it WT credentials.

A **GitHub CI world** is the existing runner guest. `wt-runner` creates it with
short-lived JIT material, runs one job, and destroys it. It has no interactive
SSH, workstation keys, devcontainer, or agent Git grant. It is never restarted,
forked, or reused.

All kinds share KVM creation, resource admission, disk ownership,
reconciliation, and cleanup. Setup, credentials, interfaces, and lifecycle
remain kind-specific and typed. Unsupported operations fail directly.

`wt-server` owns `devcontainer` and `host` worlds. `wt-runner` owns
`github-ci` worlds. They share the registry, capacity limits, libvirt code, and
image infrastructure.

Keep the common guest record for machine, disk, and capacity state. Store the
world kind and owner with it, and keep application state in kind-specific
records. Do not force interactive and CI worlds into one lifecycle enum.

Creation requests are tagged by kind. `wt new host` asks for the cloud-init
recipe instead of Git and devcontainer inputs. CI worlds are created only by
`wt-runner`.

Credential boundaries stay strict:

- Devcontainer worlds receive authorized public keys and one scoped agent Git
  grant.
- Host worlds receive authorized public keys only.
- GitHub CI worlds receive one job's JIT runner material only.

GitHub App credentials stay on the WT server. WT never supplies a developer's
private key or provider token to a world.

## Reset and protocol

Before installing this change, run `make nuke`. It destroys every WT guest and
world disk and deletes the SQLite registry and installed configuration. We do
not migrate or preserve old state.

The protocol stays at version 1. Its request, response, and registry definitions
are replaced in place. The exact client/server commit check already rejects
mixed binaries.

## Consequences

`world` no longer means only `devcontainer`. WT gets one inventory and capacity
model without pretending the three applications have the same lifecycle.

Each kind needs a real KVM lifecycle test using local fixtures, not real
developer or provider credentials. The host test verifies its cloud-init recipe
before testing stop, start, and deletion.
