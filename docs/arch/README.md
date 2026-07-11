# Architecture

Implements [plan.md](../plan.md). First iteration = **single dev + bare-metal agent only**. k8s agent is a later provider behind the same ideas—not designed in depth yet.

| Doc | Status |
|-----|--------|
| [cli.md](./cli.md) | v1 |
| [bare-metal-agent.md](./bare-metal-agent.md) | v1 |
| [k8s-agent.md](./k8s-agent.md) | deferred (stub only) |

## Iteration 1 scope

```text
Mac: wt CLI  ──HTTP/JSON──►  bare-metal agent (one host)
                              └─ libvirt VMs = worlds
                              └─ stock .devcontainer/compose inside
Mac: ssh <name>  ──────────►  guest IP (Host entry written by CLI)
```

- One developer, one (or few) fat hypervisors.  
- No multi-tenant security. No k8s. No second agent implementation.  
- Success: `wt new <repo> <name>` → `ssh <name>` → stock stack running.

## Language

**Decision: one language for CLI + agent — Rust.**

| Option | + | − |
|--------|---|---|
| **Go CLI + Rust agent** | Go ships tiny static CLIs easily; Rust for long-running agent | **Two** type systems for the same API (status enums, errors). JSON enum tagging is natural in serde; awkward and drift-prone in Go |
| **Go only** | One lang; easy CLI | Agent + libvirt/process orchestration is fine in Go, but richer domain types/enums are weaker than Rust |
| **Rust only** | Shared crates for API types (`serde`); agent gets Rust’s strengths; CLI is small enough that Rust is fine | Slightly heavier CLI toolchain/cross-compile than Go—not a real problem for v1 |

**Why not two languages for v1:** the shared surface is the product contract (`InstanceStatus`, errors, create request). Representing that twice is pure tax for a single-dev project. Only split languages if a hard constraint appears later (e.g. mandatory Go plugin host)—not for taste.

**Why Rust over Go-only:** agent is the hard, long-lived part (libvirt, provision, recipe, state). CLI is thin. Optimize for the hard part and keep types once.

Shape:

```text
crates (or workspace)
  wt-api      # shared request/response + enums (serde)
  wt          # CLI binary
  wt-agent    # bare-metal agent binary (v1)
```

Wire format: **JSON over HTTP** (or HTTPS later). Enums via serde’s usual externally/internally tagged style—defined **once** in `wt-api`.

## Shared agent contract (conceptual)

Providers implement the same verbs; v1 only ships bare-metal.

| Verb | Meaning |
|------|---------|
| create/ensure instance | source + name → world + recipe; return SSH endpoint when ready |
| list | name, status, endpoint |
| destroy | tear down world; CLI drops Host |

Status values live in `wt-api` (e.g. `Provisioning | Running | Error | Destroying`)—not stringly typed in two repos.

Auth for v1: simple (shared token or localhost/trusted network). Not a tenancy product.

## Explicitly later

- k8s agent ([k8s-agent.md](./k8s-agent.md))  
- Multi-hypervisor federation polish  
- Fancy SSH certs, multi-user IAM  

## One-line summary

**Rust monorepo: thin CLI + bare-metal libvirt agent sharing one API crate; k8s only after that path is real.**
