# wt

WT runs Ubuntu guests on KVM. Each world boots from a verified golden
image with persistent storage, managed SSH, Codex integration, and scoped Git
access.

## Use WT

- [Development and setup](./DEVELOPMENT.md)
- [Client, world lifecycle, and SSH](./docs/guides/client.md)
- [Terminal workspace](./docs/guides/shell.md)
- [Codex integration and defaults](./docs/guides/server.md#codex-defaults)
- [Server operations](./docs/guides/server.md)
- [Known limitations](./docs/known-limitations.md)

## Work on WT

- [Architecture](./docs/internals/architecture.md)
- [KVM](./docs/internals/kvm.md)
- [Provider boundaries](./docs/internals/provider.md)
- [Database](./docs/internals/database.md)
- [Rust workspace](./WORKSPACE.md)
