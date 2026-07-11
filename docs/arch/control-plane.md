# Control plane and workers

Parent: [arch README](./README.md). Plan: [plan.md](../plan.md). CLI transport: [cli.md](./cli.md).

## Roles

| Role | Job |
|------|-----|
| **Control plane** | Instance create/list/destroy; owner-scoped view. Invoked as a **remote command** over SSH in v1. |
| **Worker** | Runs worlds; ground truth for VMs/pods; inventory for the control plane. |
| **CLI** | Reaches the control plane per **context kind** (v1: SSH to site host). Context config is a sum type—[cli.md](./cli.md). |

```text
CLI  ── ssh user@host -- <helper> ──►  site host
                                         JSON in/out (wt-api)
                                         wt-local control plane + worker
                                              │
                                              ▼
                                           VMs / pods
```

**v1 transport:** remote command over SSH ([cli.md](./cli.md))—not port-forwarded HTTP.

Ground truth for “does this VM exist?” = **worker + hypervisor**.  
Mac ssh config for **worlds** is not inventory; `wt sync` projects **my** guest endpoints only.

## Auth (site access)

| Deploy | How the CLI authenticates |
|--------|---------------------------|
| **`bare_metal_ssh` context → `wt-local`** | **SSH** to the hypervisor (`ssh` + optional `identity_file`). Owner = SSH user. |
| **`k8s` context (later)** | Separate context kind; not shoehorned into SSH fields |
| Multi-node control plane later | Prefer SSH/VPN to plane host, or a new context kind if needed |

No requirement for a public control-plane HTTP port on bare metal.

## Binaries

| Binary | Status | Role |
|--------|--------|------|
| **`wt`** (crate `wt-cli`) | in workspace | CLI |
| **`wt-local`** | in workspace | Control plane + embedded worker on one host; accepts CLI via SSH |
| **`wt-control-plane`** | not built | Control plane only; workers report in |
| **`wt-worker`** | not built | Worker only |

```text
# single site
CLI ──SSH──► wt-local

# multi-node (target)
CLI ──SSH──► wt-control-plane
                    ▲ report
               wt-worker …
```

## Zombies and GC

| Situation | Handling |
|-----------|----------|
| Domain in libvirt, no instance record | Worker reconcile |
| Instance record, domain missing | Worker marks error |
| Control plane restarted | Rebuild from worker / libvirt |
| Lost laptop world Hosts | `wt sync` |

## Out of scope

- Durable central business DB  
- Billing / full IAM product  
- CLI → worker direct (bypass control plane)  
- Public internet control plane as the default for home bare metal  

## One-line summary

**CLI SSHes to the site; `wt-local` is control plane + worker; worlds get their own guest SSH Hosts via sync.**
