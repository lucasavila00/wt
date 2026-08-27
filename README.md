# wt

WT runs Ubuntu guests on KVM. Each world boots from a verified golden
image with persistent storage, managed SSH, shared terminal-pane observation,
and scoped Git
access.

WT uses three runtime-specific commands: `wt` is the user-facing client, `wts`
is the server daemon and setup tool, and `wtg` is the guest runtime baked into
golden images. See the [architecture](./docs/internals/architecture.md) for the
component mapping.

## Use WT

- [Development and setup](./DEVELOPMENT.md)
- [Client, world lifecycle, and SSH](./docs/guides/client.md)
- [Terminal workspace](./docs/guides/shell.md)
- [Codex history integration and defaults](./docs/guides/server.md#codex-defaults)
- [Server operations](./docs/guides/server.md)
- [Known limitations](./docs/known-limitations.md)

## Work on WT

- [Architecture](./docs/internals/architecture.md)
- [KVM](./docs/internals/kvm.md)
- [Provider boundaries](./docs/internals/provider.md)
- [Database](./docs/internals/database.md)
- [Rust workspace](./WORKSPACE.md)
