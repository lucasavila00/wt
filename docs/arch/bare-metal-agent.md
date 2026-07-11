# Bare-metal worker and `wt-local`

Libvirt-backed worlds on a fat host. Shipped as the embedded worker inside **`wt-local`**.  
Parent: [arch README](./README.md). Control plane: [control-plane.md](./control-plane.md). CLI: [cli.md](./cli.md). Plan: [plan.md](../plan.md).

## Deployed shape

```text
CLI ── control-plane API ──►  wt-local
                                ├─ control-plane registry
                                └─ embedded bare-metal worker
                                     libvirt guests, bootstrap, clone, compose
                                     reconcile domains vs records
```

Multi-node target: same worker logic in **`wt-worker`**, reporting to **`wt-control-plane`** (not implemented).

## World model

| Piece | Choice |
|-------|--------|
| Isolation | KVM guest per instance (libvirt) |
| Resources | Large guests OK (e.g. ≥16 GB class) |
| Image | Golden image: Linux + Docker + sshd |
| Network | Guest IP reachable from the Mac (LAN/mesh) |
| Recipe | Stock compose / `.devcontainer` in the guest |
| Trust | Trusted pool |

## Control-plane API surface

Types in `wt-api`:

| Endpoint | Behavior |
|----------|----------|
| `POST /instances` | `{ source, name, ref? }` → provision |
| `GET /instances` | list |
| `GET /instances/:name` | detail + endpoint + error |
| `DELETE /instances/:name` | destroy + free name |

Status examples: `Provisioning`, `StartingRecipe`, `Running`, `Error`, `Destroying`.

## Provision pipeline

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

## State

| State | Where |
|-------|--------|
| Instance records | `wt-local` process |
| Domains | libvirt |
| Git credentials | host env / file |

Reconcile libvirt vs records on startup and periodically (orphans).

## Ops assumptions

- Hypervisor (or nested virt), bridge/network, golden image path configured  
- Process can use libvirt  
- Mac can reach guest IPs  

## Out of scope here

- k8s worker — [k8s-agent.md](./k8s-agent.md)  
- Multi-tenant authz  

## One-line summary

**`wt-local` is the site brain and libvirt worker for single-host deploys.**
