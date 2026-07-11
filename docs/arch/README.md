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

- No listener or SSH transport. `wt` spawns `wt-local` directly. Era 1 has no guest SSH.
- Local path only. No client contexts.
- Guest: Ubuntu 24.04 + Docker Engine + Compose v2 + QEMU guest agent.

## Era 1.5

```text
wt new source name  ──►  local wt-local  ──►  KVM guest
                                                   ├─ clone → checkout → compose up
                                                   └─ sshd → shell / VS Code Remote SSH
```

- Same local transport and KVM lifecycle.
- `Running` means guest SSH and the selected Git revision's Compose project are ready.
- The checkout remains inside the guest at `/workspace/repo`; it is not exported to the host.
- Each instance records a stable SSH user, endpoint, and host-key identity. `wt sync` projects those records into managed OpenSSH files so the instance name is also the VS Code Remote SSH target.
- Guest SSH is independent of Era 2's SSH transport to `wt-local`.

## Era 2

```text
client wt  ── OpenSSH stdio ──►  site wt-local  ──►  same KVM worker
```

- Client and site are different machines.
- Same versioned helper API. No HTTP listener.

## Language and crates

**Rust** for CLI and server. Shared types in **`wt-api`** (serde JSON over stdio).

```text
crates/
  wt-api
  wt-cli       # package; binary name wt
  wt-libvirt   # production libvirt/KVM backend
  wt-local     # site helper + registry + service
  wt-setup     # Ubuntu/KVM site installer
  wt-integration-tests
```

Not in the repo yet: `wt-control-plane`, `wt-worker`.

## Control-plane API (conceptual)

| Verb | Meaning |
|------|---------|
| create | name → Docker/Compose-ready KVM world + guest IP |
| list | name, status, guest IP |
| destroy | tear down world |

Owner: local OS user.

## One-line summary

**`wt` runs local `wt-local`; `wt-libvirt` manages Docker/Compose-ready KVM worlds.**
