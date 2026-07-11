# Bare-metal worker and `wt-local`

Libvirt-backed worlds on a fat host. Shipped as the embedded worker inside **`wt-local`**.  
Parent: [arch README](./README.md). Control plane: [control-plane.md](./control-plane.md). CLI: [cli.md](./cli.md). Plan: [plan.md](../plan.md).

## Deployed shape

```text
CLI ── ssh user@hypervisor -- <wt-local helper> ──►  JSON API on host
                                                       ├─ control-plane ops
                                                       └─ embedded bare-metal worker
                                                            libvirt guests, bootstrap, clone, compose
                                                            reconcile domains vs records
```

- **v1 transport:** remote command over SSH (JSON stdio)—see [cli.md](./cli.md).  
- CLI auth = SSH to this host; **owner** = SSH user.  
- Guests get their own IPs; **`wt sync`** writes Host entries to reach **guests**, not the hypervisor API.  

Multi-node target: **`wt-worker`** + **`wt-control-plane`** (not implemented).

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

Logical ops in `wt-api` (served only via SSH remote command as that user; not a public internet listener):

| Op | Behavior |
|----|----------|
| Create | `{ source, name, ref? }` → provision; `owner` = SSH user |
| List / get | **My** instances |
| Delete | **My** instance |

Status examples: `Provisioning`, `StartingRecipe`, `Running`, `Error`, `Destroying`.  
`endpoint` on an instance is **guest** SSH (for world entry), not the hypervisor control path.

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
- Mac can **SSH to the hypervisor** (control plane)  
- Mac can reach **guest** IPs (worlds)—LAN/mesh/VPN as needed

## Out of scope here

- k8s worker — [k8s-agent.md](./k8s-agent.md)  
- Multi-tenant authz  

## One-line summary

**`wt-local` is the site brain and libvirt worker for single-host deploys.**
