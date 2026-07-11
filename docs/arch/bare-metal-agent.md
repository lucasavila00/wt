# Bare-metal worker (libvirt) + `wt-local`

Libvirt-backed worlds on a fat host. v1 ships only inside **`wt-local`** (embedded worker).  
Parent: [arch README](./README.md). Control plane: [control-plane.md](./control-plane.md). CLI: [cli.md](./cli.md). Plan: [plan.md](../plan.md). Worlds: [bare-metal-worlds](../plan-reasoning/bare-metal-worlds.md).

## Role

```text
CLI ── control-plane API ──►  wt-local
                                ├─ control-plane registry
                                └─ embedded bare-metal worker
                                     libvirt + bootstrap + clone + compose
                                     reconcile vs domains (anti-zombie)
```

Later the same worker logic runs as **`wt-worker`** (deferred binary) reporting to **`wt-control-plane`**.

## World model (v1)

| Piece | Choice |
|-------|--------|
| Isolation | **KVM guest per instance** (libvirt) |
| Resources | Large disks/RAM OK (e.g. ≥16 GB class) |
| Image | Golden qcow2/cloud: Linux + Docker + sshd |
| Network | Bridge/LAN (or mesh) so Mac can reach guest IP |
| Recipe | Stock compose / `.devcontainer` inside guest—no port rewrites |
| Trust | Single user / trusted pool |

## API (served by `wt-local` control-plane half)

Shared types in `wt-api`. Conceptual:

| Endpoint | Behavior |
|----------|----------|
| `POST /instances` | `{ source, name, ref? }` → provision |
| `GET /instances` | list |
| `GET /instances/:name` | detail + endpoint + error |
| `DELETE /instances/:name` | destroy VM + free name |

**Status enum** (example): `Provisioning`, `StartingRecipe`, `Running`, `Error`, `Destroying`.

## Provision pipeline (worker half)

```text
1. Validate name free
2. Disk from golden template
3. Inject ssh keys / cloud-init
4. libvirt define + start
5. Wait for IP + ssh
6. git clone source[@ref]
7. stock docker compose / devcontainer up
8. Running + { user, host, port=22 }
```

## State and zombies

| State | Where |
|-------|--------|
| Instance records | `wt-local` process (v1 memory); later mirrored to fleet control plane via reports |
| Actual domains | libvirt |
| Git secrets | host env / file |

Worker reconciles libvirt (`wt-*` domains) vs records on startup/periodically.

## Language

- v1 binary: **`wt-local`** (Rust)  
- Future: extract worker into lib used by `wt-worker`  
- libvirt bindings or `virsh` for v1  

## Explicitly deferred

- `wt-control-plane` / `wt-worker` processes  
- k8s worker → [k8s-agent.md](./k8s-agent.md)  
- Multi-tenant authz  

## One-line summary

**Bare-metal worlds are a worker backend; v1 users only run `wt-local`, which embeds that worker next to the control-plane API.**
