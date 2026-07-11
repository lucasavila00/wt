# Bare-metal worker and `wt-local`

Libvirt-backed worlds on a fat host. Shipped as the embedded worker inside **`wt-local`**.  
Parent: [arch README](./README.md). Control plane: [control-plane.md](./control-plane.md). CLI: [cli.md](./cli.md). Plan: [plan.md](../plan.md).

## Deployed shape

```text
CLI ── ssh user@host -- wt-local …  ──►  JSON on remote site
   └─ or  wt-local … (local)        ──►  JSON on this workstation
                                         ├─ control-plane ops
                                         └─ embedded bare-metal worker
                                              libvirt guests, bootstrap, clone, compose
```

- **v1 transport:** helper command, SSH-wrapped or local—see [cli.md](./cli.md).  
- **Owner** = SSH user or local OS user.  
- Guests: **`wt sync`** → Host entries for **guest** IPs.  

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

Logical ops in `wt-api` (helper as the invoking user; not a public internet listener):

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
| Instance records | Durable local `wt-local` registry |
| Domains | libvirt |
| Git credentials | host env / file |

Reconcile libvirt vs records on startup and periodically (orphans).

## Ops assumptions

- Hypervisor (or nested virt), bridge/network, golden image path configured  
- Process can use libvirt  
- Remote clients can **SSH to the site** host, **or** CLI runs on the site (`bare_metal_local`)  
- Client can reach **guest** IPs for world SSH (LAN/mesh/VPN/local)

## Out of scope here

- k8s worker — [k8s-agent.md](./k8s-agent.md)  
- Multi-tenant authz  

## One-line summary

**`wt-local` is the site brain and libvirt worker for single-host deploys.**
