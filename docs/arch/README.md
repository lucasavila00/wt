# Architecture

Implements [plan.md](../plan.md). Implementation order: [impl/](../impl/README.md).

| Doc | Topic |
|-----|--------|
| [cli.md](./cli.md) | `wt` CLI (contexts, API, sync, SSH) |
| [control-plane.md](./control-plane.md) | Control plane, workers, binaries |
| [bare-metal-agent.md](./bare-metal-agent.md) | Libvirt worker / `wt-local` |
| [k8s-agent.md](./k8s-agent.md) | k8s worker (not implemented) |

## Current system

```text
Mac:  wt  ── SSH (context: user@host, optional key) ──►  wt-local on site
Mac:  ssh {repo}-{feature}   after print / wt sync  →  guest world
```

- One site process: **`wt-local`** (API not exposed on the public internet by default).  
- CLI context: **SSH target**, not a control-plane URL + token.  
- Worker: stub → libvirt on the same host.  
- k8s / multi-node: target shape only for now.

## Language and crates

**Rust** for CLI and server. Shared types in **`wt-api`** (serde JSON over HTTP).

```text
crates/
  wt-api
  wt-cli       # package; binary name wt
  wt-local     # site server
```

Not in the repo yet: `wt-control-plane`, `wt-worker`.

## Control-plane API (conceptual)

| Verb | Meaning |
|------|---------|
| create | source + name → world + recipe; SSH endpoint when ready |
| list | name, status, endpoint |
| destroy | tear down world |

Auth to the site: **SSH**. Owner = SSH user. Not a separate token product for bare metal.

## One-line summary

**`wt` SSHes into the site to talk to `wt-local`; worlds are separate guest SSH Hosts.**
