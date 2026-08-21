# WT documentation

WT runs isolated, retained Ubuntu guests called worlds. Worlds boot from a
verified golden image and provide Byobu, direct SSH, scoped Git access, and
Codex integration.

## Use WT

- [Development and setup](../DEVELOPMENT.md)
- [World contract](./worlds/host.md)
- [Client and SSH](./guides/client.md), [terminal workspace](./guides/shell.md),
  and [server operations](./guides/server.md)

## Work on WT

- [Architecture](./internals/architecture.md)
- [KVM](./internals/kvm.md), [provider boundaries](./internals/provider.md),
  and [database](./internals/database.md)
- [Decision records](./adr/README.md)
