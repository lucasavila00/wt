# Implementation plan

Order of work. Product: [plan.md](../plan.md). Arch: [arch/](../arch/README.md). CLI: [arch/cli.md](../arch/cli.md).

Crates: `wt-api`, `wt-local`, `wt-libvirt`, `wt-cli` (binary `wt`), `wt-setup`, `wt-integration-tests`.

## Division of labor

| Piece | Role |
|-------|------|
| **`wt-api`** | Shared JSON request/response + status enums |
| **`wt-local`** | Site brain: helper + registry + instance service. JSON in → work → JSON out |
| **`wt-libvirt`** | Production libvirt/KVM world backend |
| **`wt-cli` (`wt`)** | Thin: spawn local helper → print |
| **`wt-setup`** | Strict Ubuntu/KVM site config + install + golden image build |
| **`wt-integration-tests`** | Injected service tests + real libvirt/KVM acceptance test |

**Real VMs (libvirt/KVM) are the hard part.**

```text
wt-cli  →  wt-local helper  →  JSON
```

## Eras

### 1 — Local loop + libvirt VMs

Ship the shape around the hard part: types + helper + CLI + real guests on **one machine**.

| Deliver | |
|---------|--|
| `wt-api` | create / list / get / delete; status; guest IP; errors |
| `wt-local` | helper entrypoint; durable local registry; owner = process user; instance service; `Provisioning` → `Running` / `Error` |
| `wt-libvirt` | Docker + Compose golden image; define/start/destroy; guest-agent readiness; guest IP; instance↔domain; KVM only |
| `wt-cli` | `new` / `ls` / `rm`; spawn helper; print status/IP |
| `wt-setup` | config-first Ubuntu install; pinned image; KVM golden build; provenance; drift checks |

**Done when:** local `wt new` creates a real KVM guest where the guest agent verifies Docker Engine + Compose; `wt ls` shows it; `wt rm` destroys it.

**Out:** clone/recipe execution, all SSH, remote contexts, multi-node, public HTTP.

#### Tests

| Lane | Covers |
|------|--------|
| **Injected worker** | helper/API, registry, ownership, state transitions, restart persistence, failures |
| **Libvirt/KVM** | production backend: image, disk, domain, boot, guest agent, Docker + Compose, IP, destroy |

Both lanes live in `wt-integration-tests`. The injected worker is fast and deterministic. It does not imitate libvirt internals.

The libvirt/KVM test is the Era 1 acceptance test: `wt new` → `wt ls` → `wt rm`.

Keep unit tests narrow: validation and wire types.

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
3. `wt-libvirt` KVM guest lifecycle + Docker/Compose readiness
4. `wt-cli` local spawn + new/ls/rm
5. Recipe + remote SSH + CLI polish

## Open (pick in code)

- Helper argv (`wt-local api` vs flags)  
- Blocking create vs poll (bites once VMs are slow)  
- Image/network layout for your lab  

## One-line summary

**Real local VM loop first → recipe + remote CLI polish; stop inventing eras for easy work.**
