# Implementation plan

Order of work. Product: [plan.md](../plan.md). Arch: [arch/](../arch/README.md). CLI: [arch/cli.md](../arch/cli.md).

Crates: `wt-api`, `wt-local`, `wt-cli` (binary `wt`).

## Division of labor

| Piece | Role |
|-------|------|
| **`wt-api`** | Shared JSON request/response + status enums |
| **`wt-local`** | Site brain: instances + worker. Helper: JSON in → work → JSON out |
| **`wt-cli` (`wt`)** | Thin: context → spawn helper (local or `ssh --`) → print / `sync` |

SSH remote spawn is a small CLI feature. **Real VMs (libvirt) are the hard part.**

```text
wt-cli  →  [optional ssh --]  wt-local helper  →  JSON
```

## Eras

### 1 — Thin local loop (stub)

Ship the shape: types + helper + CLI on **one machine**. Stub worker only—keep this short.

| Deliver | |
|---------|--|
| `wt-api` | create / list / get / delete; status; guest `endpoint`; errors |
| `wt-local` | helper entrypoint; in-memory registry; owner = process user; stub create/delete (fake endpoint OK) |
| `wt-cli` | `bare_metal_local`; `new` / `ls` / `rm`; spawn helper; print Host; basic `sync` if cheap |

**Done when:** local CLI drives helper end-to-end; ready to swap stub for libvirt without redesigning wire types or CLI.

**Out:** libvirt, compose, remote SSH (unless free), multi-node, public HTTP.

---

### 2 — Libvirt VMs

**Main risk era.** Real guests you can SSH into.

| Deliver | |
|---------|--|
| `wt-local` | golden image/template; define/start/destroy; wait for IP; inject keys; instance↔domain; `Provisioning` → `Running` / `Error` |
| ops | image, pool, network/bridge, libvirt permissions |
| CLI | unchanged path; print/sync hit real endpoints |

**Done when:** `wt new` → print/`sync` → `ssh {repo}-{feature}` works; `wt rm` destroys the domain.

**Out:** full app compose (empty/docker-ready guest is enough).

Also here if it unblocks you: sqlite so instance records survive helper restarts while VMs still exist.

---

### 3 — Stock recipe + client completeness

Everything that makes it a daily driver **after** VMs work—one era, not split for ceremony.

| Area | Deliver |
|------|---------|
| **Recipe** | clone source[@ref]; detect compose / `.devcontainer`; stock `compose up`; phases / `last_error` |
| **`bare_metal_ssh`** | same helper under `ssh user@host -- …` |
| **CLI** | context list/use/show; solid `sync`; wait/poll if create is slow; clearer errors; optional `wt ssh` |
| **Guest access** | known_hosts / key polish |

**Done when:** real repo comes up on `new`; Mac→remote hypervisor works like local workstation; day-to-day commands are usable.

---

### Later (not an era until needed)

- Modules/bins for multi-node (`wt-control-plane`, `wt-worker`)  
- k8s context kind  
- Runbook polish, destroy policies, fleets  

Do not pre-build these.

---

## First commits

1. `wt-api` types  
2. `wt-local` helper + memory stub  
3. `wt-cli` local spawn + new/ls/rm (+ print/sync)  
4. **Libvirt** (prioritize once the loop exists)  
5. Recipe + remote SSH + CLI polish  

## Open (pick in code)

- Helper argv (`wt-local api` vs flags)  
- Blocking create vs poll (bites once VMs are slow)  
- Image/network layout for your lab  

## One-line summary

**Short stub loop → libvirt (hard) → recipe + remote CLI polish; stop inventing eras for easy work.**
