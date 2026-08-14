# WT documentation

WT runs isolated KVM guests called worlds.

| Kind | Use | Lifetime | Access |
|------|-----|----------|--------|
| `devcontainer` | Repository development | Named, retained | Byobu, guest SSH, app SSH |
| `host` | Raw Ubuntu with cloud-init | Named, retained | Byobu and direct guest SSH |
| `github-ci` | GitHub Actions job foundation | One job | None |

Start with [Worlds](./worlds/README.md), then read the page for the kind you
need:

- [Devcontainer](./worlds/devcontainer.md)
- [Host](./worlds/host.md)
- [GitHub CI](./worlds/github-ci.md)

Devcontainer and host worlds are available through the installed server. The
GitHub CI lifecycle and registry kind exist, but its operator service and image
installer are not implemented yet.

User guides: [client](./guides/client.md) and [server](./guides/server.md).

Implementation: [architecture](./internals/architecture.md),
[KVM](./internals/kvm.md), [provider boundaries](./internals/provider.md),
[database](./internals/database.md), and
[registry cache](./internals/registry-cache.md).

Decisions are recorded in [ADRs](./adr/).
