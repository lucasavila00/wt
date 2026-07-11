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
Client:  wt  ── ssh user@host -- helper  ──►  wt-local (remote site)
         wt  ── helper (no ssh)          ──►  wt-local (this workstation)
Client:  ssh {repo}-{feature}  after print / wt sync  →  guest world
```

- Site binary: **`wt-local`** (not a public HTTP API).  
- CLI contexts: **`bare_metal_ssh`** and **`bare_metal_local`** (same helper JSON); later **`k8s`**.  
- Worker: stub → libvirt on the site host.

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

Auth: SSH user (remote) or local OS user (workstation). Not a separate token product for bare metal.

## One-line summary

**`wt` runs the `wt-local` helper (via SSH or locally); worlds are separate guest SSH Hosts.**
