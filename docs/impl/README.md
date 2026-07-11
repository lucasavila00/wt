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

### 1 — Local loop + libvirt VMs

Ship the shape around the hard part: types + helper + CLI + real guests on **one machine**.

| Deliver | |
|---------|--|
| `wt-api` | create / list / get / delete; status; guest `endpoint`; errors |
| `wt-local` | helper entrypoint; durable local registry; owner = process user; golden image/template; define/start/destroy; wait for IP; inject keys; instance↔domain; `Provisioning` → `Running` / `Error` |
| `wt-cli` | `bare_metal_local`; `new` / `ls` / `rm`; spawn helper; print Host; basic `sync` if cheap |
| ops | image, pool, network/bridge, libvirt permissions |

**Done when:** local `wt new` → print/`sync` → `ssh {repo}-{feature}` reaches a real Docker-ready guest; `wt rm` destroys the domain.

**Out:** compose, remote SSH (unless free), multi-node, public HTTP.

#### Tests

| Lane | Covers |
|------|--------|
| **Injected worker** | helper/API, registry, ownership, state transitions, restart/reconcile, failures |
| **QEMU/libvirt** | image, disk, domain, boot, network, IP discovery, SSH, destroy |

The injected worker is fast and deterministic. It does not imitate libvirt internals.

The QEMU test is the Era 1 acceptance test: `wt new` → `wt ls` → guest SSH → `wt rm`. Run with software emulation first; use KVM acceleration when available. Same test.

Keep unit tests narrow: validation, wire types, Host rendering.

---

### 2 — Stock recipe + client completeness

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
2. `wt-local` helper + durable local registry
3. `wt-cli` local spawn + new/ls/rm (+ print/sync)
4. **Libvirt** guest lifecycle + SSH endpoint
5. Recipe + remote SSH + CLI polish

## Open (pick in code)

- Helper argv (`wt-local api` vs flags)  
- Blocking create vs poll (bites once VMs are slow)  
- Image/network layout for your lab  

## One-line summary

**Real local VM loop first → recipe + remote CLI polish; stop inventing eras for easy work.**
