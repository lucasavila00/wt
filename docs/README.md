# WT documentation

WT runs isolated KVM guests called worlds.

| Kind | Use | Lifetime | Access |
|------|-----|----------|--------|
| `devcontainer` | Repository development | Named, retained | Byobu, guest SSH, app SSH |
| `host` | Raw Ubuntu with cloud-init | Named, retained | Byobu and direct guest SSH |
| `github-ci` | GitHub Actions job foundation | One job | None |

Devcontainer and host worlds are available now. GitHub CI has lifecycle and
registry code, but no operator service or image installer yet.

## Use WT

- [Development and setup](../DEVELOPMENT.md)
- [World contracts](./worlds/README.md): [devcontainer](./worlds/devcontainer.md),
  [host](./worlds/host.md), and [GitHub CI](./worlds/github-ci.md)
- [Client and SSH](./guides/client.md), [terminal workspace](./guides/shell.md),
  and [server operations](./guides/server.md)

## Work on WT

- [Architecture](./internals/architecture.md)
- [KVM](./internals/kvm.md), [provider boundaries](./internals/provider.md),
  [database](./internals/database.md), and
  [registry cache](./internals/registry-cache.md)
- [Decision records](./adr/README.md)
