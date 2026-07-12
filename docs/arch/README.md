# Architecture

Current product direction: [plan.md](../plan.md).

| Doc | Topic |
|-----|-------|
| [cli.md](./cli.md) | `wt` CLI, contexts, and SSH inventory |
| [bare-metal-agent.md](./bare-metal-agent.md) | Libvirt/KVM world backend |
| [registry-cache.md](./registry-cache.md) | Shared Docker/OCI pull cache for KVM worlds |

## System

```text
client wt
   ├─ local context ─────────► wt-server api
   └─ OpenSSH context ───────► wt-server api
                                  ├─ SQLite registry
                                  └─ wt-libvirt ──► KVM world
                                                        ├─ Git checkout
                                                        ├─ devcontainer
                                                        └─ guest SSH
```

The client reads named contexts from `~/.wt/config.toml`. A local context runs
`wt-server api` from `PATH`; an SSH context runs the same helper on a server through
stock OpenSSH. Both transports carry one versioned JSON request and response over
stdio. There is no public control-plane listener.

Each server runs on Ubuntu 24.04 amd64 with KVM and libvirt. `wt-server` scopes its
registry to the OS user executing the helper, and `wt-libvirt` creates one guest
per world. A world is `Running` only after guest SSH, the selected Git revision,
and the repository's stock devcontainer are ready.

The checkout remains inside the guest at `/workspace`. Each world records a
stable SSH user, endpoint, and unique public host keys. `wt sync` projects that
inventory into managed app-container and guest-host aliases without editing the
user's main SSH config.

Git sources are SSH-only. Each server supplies an encrypted Git identity and a
known-hosts file. `wt new` reads the passphrase on the client terminal and sends
it through the local/OpenSSH helper request for the blocking clone. The encrypted
identity and trust bundle are copied into the trusted world's checkout for Git
from both the guest and devcontainer; the passphrase is not persisted.
Client-to-server OpenSSH authentication is a separate role, though deployments
may configure the same identity for both roles.

## Language and crates

Rust workspace:

```text
crates/
  wt-api                shared JSON types
  wt-cli                package for the wt binary
  wt-command            shared local process command builder
  wt-guest              injected persistent app-session helpers
  wt-libvirt            production libvirt/KVM backend
  wt-server              server helper, registry, and service
  wt-server-setup        Ubuntu/KVM installer and image builder
  wt-integration-tests  injected and real-system tests
```

## Control-plane API

| Operation | Meaning |
|-----------|---------|
| create | Provision a devcontainer-ready KVM world |
| list | Return the caller's worlds and access inventory |
| get | Return one caller-owned world |
| delete | Destroy one caller-owned world |

The API uses protocol version 1 JSON over helper stdio. The owner is the OS user
running `wt-server`, whether the helper was started locally or through OpenSSH.
