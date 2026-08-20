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
Docker does not apply a seccomp profile because the KVM guest, rather than the
containers inside it, is the profiling boundary.

The retained guest image supplies the `wt` login at UID/GID `1001:1001` and
the shared Byobu/tmux foundation. Shared provisioning installs the requested
Git author for `wt`; repository setup records the same author locally so it
follows the checkout into the application container. The repository's
`remoteUser` is a separate container contract; WT does not assume that it is
`wt`.

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

## Shared agent conversations

When the server enables the example shared folders, they are mounted in the VM
at:

```text
/home/wt/.codex/sessions
/home/wt/.claude/projects
```

WT does not inject them into the devcontainer. A repository using Docker
Compose can expose them to its configured `remoteUser` with service bind
mounts. For a container whose user is `vscode`:

```yaml
services:
  app:
    volumes:
      - /home/wt/.codex/sessions:/home/vscode/.codex/sessions
      - /home/wt/.claude/projects:/home/vscode/.claude/projects
```

The repository owns the container target paths and permissions; WT does not
assume every image uses `/home/vscode`.

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
