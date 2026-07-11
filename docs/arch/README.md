# Architecture

Implements [plan.md](../plan.md). Implementation order: [impl/](../impl/README.md).

| Doc | Topic |
|-----|--------|
| [cli.md](./cli.md) | Era 1 local `wt` CLI |
| [control-plane.md](./control-plane.md) | Control plane, workers, binaries |
| [bare-metal-agent.md](./bare-metal-agent.md) | Libvirt worker / `wt-local` |
| [k8s-agent.md](./k8s-agent.md) | k8s worker (not implemented) |

## Era 1

```text
wt  ── local stdio ──►  wt-local  ──►  wt-libvirt  ──►  KVM world
```

- No listener. No SSH. `wt` spawns `wt-local` directly.
- Context: `bare_metal_local` only.
- Guest: Ubuntu 24.04 + Docker Engine + Compose v2 + QEMU guest agent.

## Language and crates

**Rust** for CLI and server. Shared types in **`wt-api`** (serde JSON over stdio).

```text
crates/
  wt-api
  wt-cli       # package; binary name wt
  wt-libvirt   # production libvirt/KVM backend
  wt-local     # site helper + registry + service
  wt-integration-tests
```

Not in the repo yet: `wt-control-plane`, `wt-worker`.

## Control-plane API (conceptual)

| Verb | Meaning |
|------|---------|
| create | source + name → Docker/Compose-ready KVM world + guest IP |
| list | name, status, guest IP |
| destroy | tear down world |

Owner: local OS user.

## One-line summary

**`wt` runs local `wt-local`; `wt-libvirt` manages Docker/Compose-ready KVM worlds.**
