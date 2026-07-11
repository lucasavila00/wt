# Architecture

Current product direction: [plan.md](../plan.md).

| Doc | Topic |
|-----|-------|
| [cli.md](./cli.md) | `wt` CLI, contexts, and SSH inventory |
| [control-plane.md](./control-plane.md) | Site helper, registry, and API |
| [bare-metal-agent.md](./bare-metal-agent.md) | Libvirt/KVM world backend |
| [k8s-agent.md](./k8s-agent.md) | Unimplemented backend stub |

## System

```text
client wt
   ├─ local context ─────────► wt-local api
   └─ OpenSSH context ───────► wt-local api
                                  ├─ SQLite registry
                                  └─ wt-libvirt ──► KVM world
                                                        ├─ Git checkout
                                                        ├─ devcontainer
                                                        └─ guest SSH
```

The client reads named contexts from `~/.wt/config.toml`. A local context runs
`wt-local api` from `PATH`; an SSH context runs the same helper on a site through
stock OpenSSH. Both transports carry one versioned JSON request and response over
stdio. There is no public control-plane listener.

Each site runs on Ubuntu 24.04 amd64 with KVM and libvirt. `wt-local` scopes its
registry to the OS user executing the helper, and `wt-libvirt` creates one guest
per world. A world is `Running` only after guest SSH, the selected Git revision,
and the repository's stock devcontainer are ready.

The checkout remains inside the guest at `/workspace`. Each world records a
stable SSH user, endpoint, and unique public host keys. `wt sync` projects that
inventory into managed app-container and guest-host aliases without editing the
user's main SSH config.

Git sources are SSH-only. Each site supplies a dedicated unencrypted Git identity
and known-hosts file. The identity and trust bundle are copied into the trusted
world's checkout for Git from both the guest and devcontainer. Client-to-site
OpenSSH authentication is separate.

## Language and crates

Rust workspace:

```text
crates/
  wt-api                shared JSON types
  wt-cli                package for the wt binary
  wt-guest              injected wt-app-shell helper
  wt-libvirt            production libvirt/KVM backend
  wt-local              site helper, registry, and service
  wt-local-setup        Ubuntu/KVM installer and image builder
  wt-integration-tests  injected and real-system tests
```

## Control-plane API

| Operation | Meaning |
|-----------|---------|
| create | Provision a devcontainer-ready KVM world |
| list | Return the caller's worlds and access inventory |
| get | Return one caller-owned world |
| delete | Destroy one caller-owned world |

The API uses protocol version 3 JSON over helper stdio. The owner is the OS user
running `wt-local`, whether the helper was started locally or through OpenSSH.
