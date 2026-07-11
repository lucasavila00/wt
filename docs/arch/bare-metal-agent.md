# Bare-metal agent

v1 world factory: runs on the hypervisor host (or beside it with access to libvirt).  
Parent: [arch README](./README.md). CLI: [cli.md](./cli.md). Plan: [plan.md](../plan.md). Worlds: [bare-metal-worlds](../plan-reasoning/bare-metal-worlds.md).

## Role

```text
CLI ──HTTP──► agent
                ├─ track instances (name → VM + status + IP)
                ├─ libvirt: define/start/destroy guests
                ├─ bootstrap: user, ssh key, docker (golden image)
                └─ in guest: clone source, stock compose/devcontainer up
```

One agent process per site is enough for a single dev (one fat server). Multi-host inventory can wait.

## World model (v1)

| Piece | Choice |
|-------|--------|
| Isolation | **KVM guest per instance** (libvirt) |
| Resources | Large disks/RAM OK (e.g. ≥16 GB class); not optimizing density |
| Image | Golden qcow2/cloud image: Linux + Docker + sshd (+ byobu later) |
| Network | Bridge/LAN so guest gets an IP the Mac can reach (or VPN/tailnet later) |
| Recipe | Clone git source into guest; run **stock** compose from `.devcontainer` / compose files—no port rewrites |
| Trust | Single user / trusted; no neighbor sandboxing |

## API (v1 surface)

Shared types in `wt-api` (Rust/serde). Conceptual:

| Endpoint | Behavior |
|----------|----------|
| `POST /instances` | body: `{ source, name, ref? }` → start async provision; return id/name + status |
| `GET /instances` | list |
| `GET /instances/:name` | detail + endpoint + last error |
| `DELETE /instances/:name` | destroy VM + free name |

**Status enum** (example): `Provisioning`, `StartingRecipe`, `Running`, `Error`, `Destroying`. Serde once; CLI displays the same.

Create is **not** required to be fully idempotent in v1; document “name must be free” until later.

## Provision pipeline (inside agent)

```text
1. Validate name free
2. Clone/copy golden disk or create from template
3. Inject: hostname, ssh authorized_keys, optional cloud-init user-data
4. libvirt define + start
5. Wait for IP + ssh accepting
6. ssh/exec: git clone <source> [@ref]
7. ssh/exec: detect compose/devcontainer; docker compose up (stock)
8. Mark Running; expose { user, host, port=22 }
```

On failure: mark `Error` with message; leave VM for debug or auto-destroy policy (pick one simple rule for v1—e.g. leave until `DELETE`).

## State the agent owns

| State | Store (sketch) |
|-------|----------------|
| Instance records | Local sqlite/json on agent host |
| libvirt domains | libvirt |
| Secrets for git | Agent env / host file (deploy key)—v1 pragmatic |

Mac does not hold source of truth for “what exists.”

## Language / process

- **Rust** binary `wt-agent`  
- libvirt: safe bindings or shell out to `virsh` for v1 if faster to stabilize—prefer real API once paths work  
- Long-running HTTP server  
- Concurrency: one provision at a time is OK for single-dev v1; queue or lock per name

## Ops assumptions (v1)

- Operator prepared: nested virt or bare metal, bridge network, golden image path configured  
- Agent runs as a user that can talk to libvirt  
- Mac reaches guest IPs (same L2/L3 or mesh)

## Explicitly not this agent

- k8s / DinD pods → [k8s-agent.md](./k8s-agent.md)  
- Port remapping on a shared Docker host  
- Multi-tenant authz  

## One-line summary

**Rust agent on the hypervisor: libvirt VM per name, stock compose inside, HTTP API + enums shared with the CLI.**
