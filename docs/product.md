# Product

WT manages named, parallel instances of an existing devcontainer recipe. Each
instance is a KVM world with its own network, checkout, Docker daemon, and stock
devcontainer.

## Interface

- `wt new SOURCE NAME`: create a world from an SSH Git source.
- `wt logs NAME`: replay and follow provisioning output.
- `wt ls`: list worlds across configured contexts.
- `wt rm NAME`: destroy a world.
- `wt sync`: update managed OpenSSH aliases.
- `ssh NAME`: enter the persistent app session.
- `ssh NAME-dc`: enter the devcontainer directly.
- `ssh NAME-host`: enter the guest.

`context.world` is the stable name. Short names work when globally unique.

## Constraints

- Ubuntu 24.04 amd64 servers with KVM; no emulation fallback.
- SSH Git sources only.
- The repository's `devcontainer.json` remains the recipe contract.
- App images must be Debian- or Ubuntu-derived and support `apt`.
- Servers are trusted credential boundaries, not hostile multi-tenant systems.
- No public control-plane listener or runtime configuration overrides.
- Install input materializes `/etc/wt/server.toml`; drift fails.

WT is not a CI system, Git worktree manager, recipe language, or hosted IDE.

See the [architecture](./arch/README.md).
