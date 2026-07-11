# Architecture

Implements [plan.md](../plan.md). First iteration = **single dev + `wt-local` only**. Multi-node control plane / k8s worker deferred.

| Doc | Status |
|-----|--------|
| [cli.md](./cli.md) | v1 |
| [control-plane.md](./control-plane.md) | v1 model + deferred fleet bins |
| [bare-metal-agent.md](./bare-metal-agent.md) | libvirt worker (embedded in `wt-local`) |
| [k8s-agent.md](./k8s-agent.md) | deferred stub |

## Iteration 1 scope

```text
Mac: wt CLI  ── control-plane API ──►  wt-local  (plane + embedded worker)
Mac: ssh <name>  after pasting printed Host (auto-edit later)
```

- One developer, one fat hypervisor: run **`wt-local`**.  
- CLI points at **one URL** = `wt-local`.  
- No separate central DB/Redis. No k8s.  
- Success path grows by era ([impl](../impl/README.md)).

## Language

**Rust only** for CLI + server(s)—shared `wt-api` types (serde). No Go+Rust split.

```text
crates
  wt-api       # shared control-plane types
  wt           # CLI binary
  wt-local     # v1: control plane + embedded worker

# deferred bins (not in workspace yet)
  wt-control-plane
  wt-worker
```

Wire format: **JSON over HTTP**.

## Shared control-plane contract (conceptual)

| Verb | Meaning |
|------|---------|
| create/ensure instance | source + name → world + recipe; SSH endpoint when ready |
| list | name, status, endpoint |
| destroy | tear down via owning worker |

Auth for v1: simple shared token or trusted network. Not a tenancy product.

## Explicitly later

- `wt-control-plane` + `wt-worker` binaries ([control-plane.md](./control-plane.md))  
- k8s worker ([k8s-agent.md](./k8s-agent.md))  
- Fancy SSH certs, multi-user IAM  

## One-line summary

**`wt` → control-plane API; v1 server is `wt-local`; fleet splits into `wt-control-plane` + `wt-worker` later without changing the CLI contract.**
