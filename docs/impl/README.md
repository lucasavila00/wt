# Implementation plan

How we build, in order. Product/arch: [plan.md](../plan.md), [arch/](../arch/README.md).  
Workspace: `crates/wt-api`, `wt`, `wt-local`.

## Approach: thin end-to-end, then thicken

Full stack on day one (libvirt + golden image + clone + compose + SSH Host) is possible but **slow to first “it works”** and hard to debug.

**Preferred:** one **vertical slice** that exercises the real product gesture as early as possible, with **fake or minimal worlds**, then replace the fake with real layers one by one.

```text
Era 1:  CLI ↔ wt-local  (control plane + stub worker)
Era 2:  real SSH-able guest                (libvirt / golden image)
Era 3:  stock recipe in guest              (clone + compose)
Era 4:  status, errors, ops UX             (make it daily-driver)
Era 5:  harden + library seams             (ready for wt-control-plane / wt-worker)
```

Control plane model: [arch/control-plane.md](../arch/control-plane.md).  
v1 binary: **`wt-local`** only—not `wt-control-plane` + `wt-worker`.

Gesture we protect every era:

```text
wt new <source> <name>  →  control plane has instance  →  CLI prints SSH Host snippet
wt ls / wt rm
# later (stable path): CLI applies snippet to managed ssh config → ssh <name>
```

**SSH config auto-edit is not Era 1.** First only **print** the `Host` block. Applying edits is a **later smoothness feature**.

---

## Era 1 — Contract + `wt-local` skeleton

**Delivers:** CLI against `wt-local`; instances in process state; CLI **prints** the SSH config delta; shared types are real.

| Layer | What ships |
|-------|------------|
| **wt-api** | Create/list/get/delete types; `InstanceStatus` enum; serde JSON |
| **wt-local** | HTTP control-plane server; in-memory registry; **stub embedded worker** (no libvirt) |
| **wt** | `new` / `ls` / `rm`; config (**control-plane URL** → `wt-local`); **print** Host add/remove—no file edits |

**Done when:**

- `cargo run -p wt-local` + `cargo run -p wt -- new …` creates a row  
- `wt ls` / `wt rm` work; Host snippet printed  
- Types only in `wt-api`  
- Internal modules can still say control-plane vs worker; **one binary** ships  

**Explicitly not:** auto ssh config, VMs, `wt-control-plane` / `wt-worker` bins, compose, byobu.

---

## Era 2 — Real world shell (libvirt + SSH)

**Delivers:** `ssh <name>` lands on a **real guest** `wt-local` created.

| Layer | What ships |
|-------|------------|
| **wt-local** | libvirt define/start/destroy; golden image; wait for IP; SSH key; `Provisioning` → `Running` |
| **wt** | same commands; printed Host points at real guest |
| **ops** | golden image + bridge docs (single-dev) |

**Done when:** `wt new …` → paste Host → `ssh <name>` works; `wt rm` destroys domain.

**Explicitly not:** full app recipe (optional manual compose inside).

---

## Era 3 — Stock recipe inside the world

**Delivers:** clone + stock compose/devcontainer on create.

| Layer | What ships |
|-------|------------|
| **wt-local** | after SSH up: clone; detect compose/`.devcontainer`; `docker compose up` stock |
| **wt-api** | ref, last_error, recipe phase as needed |
| **wt** | pass source/ref; surface errors |

**Done when:** real repo `new` → stack up; failures → `Error` + `rm` still cleans.

---

## Era 4 — Daily-driver UX

| Area | Examples |
|------|----------|
| Status | Poll/wait phases |
| Config | config.toml, tokens, known_hosts |
| **SSH config apply** | when stable: managed Include / write on `new` |
| Landing | byobu / docker exec feel |
| Robustness | timeouts, destroy policy, clear `ls` errors |

**Done when:** real side project without hand-holding; auto Host only after print path is trusted.

---

## Era 5 — Harden + extractable seams

**Delivers:** libraries such that **`wt-control-plane`** and **`wt-worker`** can become thin bins later—without shipping them yet.

| Layer | What ships |
|-------|------------|
| **libs** | control-plane + worker traits/modules used by `wt-local` |
| **docs / API** | stable control-plane HTTP contract; worker id fields ready for fleet list |
| **ops** | runbook; control-plane state disposable if worker can re-report |

**Explicitly not this era:** releasing multi-node binaries or k8s worker ([k8s-agent.md](../arch/k8s-agent.md)).

---

## Why this order

| Risk if you invert | Mitigation |
|--------------------|------------|
| Libvirt for weeks, no CLI | Era 1 first |
| CLI against nothing | `wt-local` is real HTTP from Era 1 |
| Compose before SSH | Era 2 then 3 |
| Abstract fleet too early | One binary until multi-host hurts |

```text
Era 1 E2E:  name on wt-local + printed Host
Era 2 E2E:  + real ssh
Era 3 E2E:  + real stack
Era 4 E2E:  + not annoying
Era 5 E2E:  + not a dead-end for wt-control-plane / wt-worker
```

---

## Suggested first coding slice (Era 1)

1. `wt-api`: request/response + `InstanceStatus`  
2. `wt-local`: listen, in-memory CRUD, stub worker module  
3. `wt`: subcommands + client + **print** SSH snippet  
4. Manual smoke: start `wt-local`, `new` / `ls` / `rm`  

No libvirt until Era 1 feels boring.

---

## Open (not blocking Era 1)

- Async create vs blocking `new`  
- Memory vs sqlite on `wt-local`  
- Compose detection order — Era 3  

## One-line summary

**Five eras on `wt` + `wt-local`: skeleton → SSH VM → recipe → UX → library seams; fleet bins stay deferred until needed.**
