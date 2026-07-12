# Plan

Architecture: [arch/](./arch/README.md). Background:
[plan-reasoning/](./plan-reasoning/).

## Current product

`wt` manages named parallel instances of an existing devcontainer recipe. The
client is a thin cockpit; repositories, Docker, and devcontainers run inside KVM
worlds on Ubuntu servers.

- `wt new <source> <name> [--ref <ref>]` creates a world from an SSH Git source.
- Each world has its own VM, network identity, checkout, and stock devcontainer.
- Named local and OpenSSH contexts carry the same versioned `wt-server` JSON API.
- `context.world` is the stable name; short names work when globally unique.
- `wt sync` creates strict managed OpenSSH aliases for the app container and guest.
- App aliases always attach to one persistent, multiplexed tmux session per world.
- Servers are trusted credential boundaries, not hostile multi-tenant sandboxes.
- There is no public control-plane listener and no runtime configuration override;
  each server uses the complete strict config at `/etc/wt/server.toml`.

## Product constraints

- The repository's stock `devcontainer.json` is the recipe contract. WT adds no
  repository configuration, generated override, or path rewriting.
- Each world owns a network namespace, so stock published ports work across
  parallel instances without application-specific port matrices.
- Git checkout and credentials remain inside the trusted world. Client-to-server
  authentication remains owned by OpenSSH.
- KVM is required for the bare-metal backend; there is no emulation fallback.
- WT is an interactive world manager, not a CI system, Git worktree manager,
  recipe language, or hosted development environment.
