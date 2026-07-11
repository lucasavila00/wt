# Architecture

Implements [plan.md](../plan.md). Implementation order: [impl/](../impl/README.md).

| Doc | Topic |
|-----|--------|
| [cli.md](./cli.md) | `wt` CLI |
| [control-plane.md](./control-plane.md) | Control plane, workers, binaries |
| [bare-metal-agent.md](./bare-metal-agent.md) | Libvirt worker / `wt-local` |
| [k8s-agent.md](./k8s-agent.md) | k8s worker (not implemented) |

## Current system

```text
Mac:  wt  ── control-plane API ──►  wt-local  (control plane + embedded worker)
Mac:  ssh <name>   after applying the printed Host snippet
```

- One site process: **`wt-local`**.  
- CLI config: control-plane URL → that process.  
- Worker backend today: stub → then libvirt on the same host.  
- k8s worker and multi-node binaries: specified for the target shape, not implemented.

## Language and crates

**Rust** for CLI and server. Shared types in **`wt-api`** (serde JSON over HTTP).

```text
crates/
  wt-api
  wt           # CLI
  wt-local     # site server
```

Not in the repo yet: `wt-control-plane`, `wt-worker`.

## Control-plane API (conceptual)

| Verb | Meaning |
|------|---------|
| create | source + name → world + recipe; SSH endpoint when ready |
| list | name, status, endpoint |
| destroy | tear down world |

Auth: simple token or trusted network. Not a tenancy product.

## One-line summary

**`wt` talks only to a control-plane URL; that URL is `wt-local` on a single site.**
