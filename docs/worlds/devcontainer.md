# Devcontainer worlds

A devcontainer world is a retained development environment for one repository
and base branch.

`wt new` asks for the context, name, Git source, base branch, resources, and
confirmation. It creates an Ubuntu guest, checks out the repository at
`/workspace`, and starts its devcontainer recipe. The repository remains the
environment contract.

Each world has its own KVM guest, disk, Docker daemon, checkout, containers,
Byobu session, guest SSH identity, and app SSH identity. Compose recipes are
supported; the primary devcontainer is the container an editor would open.

## Access

| Alias | Target |
|-------|--------|
| `CONTEXT.NAME` | Persistent Byobu session whose panes enter the primary container |
| `CONTEXT.NAME-vs` | Direct app SSH through the guest; used by editors and SFTP |
| `CONTEXT.NAME-host` | Direct guest SSH for recovery |

Short aliases exist only when the name is unique across contexts. WT pins both
guest and app host keys. `wt code NAME` resolves the live workspace mount and
opens the `-vs` alias with VS Code Remote-SSH. `wt ssh NAME` refreshes the
managed aliases and connects to the qualified persistent Byobu alias.

The first `ssh CONTEXT.NAME` completes setup. It clones the repository, starts
the recipe, and leaves its output in Byobu and the guest setup log. Later
connections attach to the same session.

## Git

The server holds provider credentials. A world receives a revocable grant that
can read every available repository and write only branches under `wt/`.
Provider keys and tokens do not enter the guest or container.

Configured provider URLs route through the guest relay. Normal Git uses the
gateway automatically, and `ag-git` selects the repository from the current
checkout. The grant is revoked before the world disk is deleted.

## Requirements

- The repository contains `.devcontainer/devcontainer.json`.
- The recipe sets an existing `remoteUser`.
- App images use Ubuntu 24.04 or newer, or Debian 13 or newer, with `apt`.
- Repository and devcontainer code are trusted inside their world.

Stopping preserves the writable disk. `wt start NAME` boots the guest, restores
its containers, and verifies guest and app SSH identity.
