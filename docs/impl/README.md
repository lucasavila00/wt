# Implementation plan

Order of work. Product: [plan.md](../plan.md). Arch: [arch/](../arch/README.md). CLI: [arch/cli.md](../arch/cli.md).

Crates: `wt-api`, `wt-local`, `wt-cli` (binary `wt`).

## Division of labor

| Piece | Role |
|-------|------|
| **`wt-api`** | Shared JSON request/response + status enums |
| **`wt-local`** | Site brain: instance state, worker (stub→libvirt→recipe). **Helper command**: JSON in → work → JSON out |
| **`wt-cli` (`wt`)** | Thin: read context → build argv → exec helper → print / `sync`. **No** provision logic |

```text
wt-cli  →  [optional ssh --]  wt-local helper  →  JSON
```

| Context | Spawn |
|---------|--------|
| `bare_metal_local` | `wt-local …` on this machine |
| `bare_metal_ssh` | `ssh user@host -- wt-local …` |

Same helper either way. CLI without server is useless; server without *some* driver is awkward—so **develop them together**, with **local** as the default dev loop (no SSH required on an Ubuntu workstation).

## Approach

One vertical slice early: types + helper + thin CLI on **one machine**. Then deepen the worker. Remote SSH is a small spawn variant, not a separate product phase.

```text
Era 1:  wt-api + wt-local helper + wt-cli (bare_metal_local) — stub worlds
Era 2:  bare_metal_ssh spawn (same helper, wrap in ssh)
Era 3:  libvirt guests + real guest endpoints
Era 4:  stock clone + compose in guest
Era 5:  UX polish + seams for multi-node later
```

Gesture each era protects (once CLI exists):

```text
wt new / ls / rm / sync   →  helper JSON  →  print or managed Hosts
```

---

## Era 1 — Local end-to-end skeleton

**Goal:** on one box (e.g. Ubuntu workstation), install/run **both** binaries; drive stub instances with the real CLI path.

| Deliver | Detail |
|---------|--------|
| **`wt-api`** | Create/list/get/delete types; `InstanceStatus`; optional guest `endpoint`; errors |
| **`wt-local`** | Helper entrypoint (e.g. `wt-local api` or stdin JSON); in-memory registry; owner = process user; **stub** create (record source/name, fake endpoint OK) |
| **`wt-cli`** | Context sum type with **`bare_metal_local`**; commands `context`, `new`, `ls`, `rm`, `sync`; exec helper; print Host snippet; write managed ssh_config from list |

**Dev loop:** edit crates → `cargo run -p wt-local` / `cargo run -p wt-cli` on the same machine → no SSH, no second host.

**Done when:**

- `wt` with `bare_metal_local` can new/ls/rm/sync against local helper  
- Types only in `wt-api`  
- CLI has no business logic beyond spawn + format + sync files  

**Not in this era:** real libvirt, compose, multi-node, public HTTP, requirement to use SSH.

Optional: you can still smoke the helper with raw shell (`echo … \| wt-local …`) while debugging, but **Era 1 is not “server without CLI.”**

---

## Era 2 — Remote spawn (`bare_metal_ssh`)

**Goal:** same CLI + helper, Mac (or laptop) → remote Ubuntu via SSH.

| Deliver | Detail |
|---------|--------|
| **`wt-cli`** | Implement **`bare_metal_ssh`** context: build `ssh [-i] [-p] user@host -- wt-local …` with same JSON as local |
| **Site** | `wt-local` installed on remote; SSH login works as the owner user |
| **Docs** | Example context for remote lab |

**Done when:** Mac `wt new/ls/rm/sync` against remote host matches local behavior; owner = SSH user.

**Not in this era:** libvirt (still stub unless already added).

---

## Era 3 — Libvirt worlds

**Goal:** helper creates real guests; guest SSH works after sync.

| Deliver | Detail |
|---------|--------|
| **`wt-local` only** | Golden image, define/start/destroy, wait for IP, inject keys, real `endpoint` |
| **CLI** | Unchanged spawn paths; print/sync become useful for real Hosts |
| **Ops** | Image, network, install notes (local workstation and/or remote hypervisor) |

**Done when:** `wt new` → `wt sync` → `ssh {repo}-{feature}` into guest; `wt rm` destroys domain. Works for both local and SSH contexts if libvirt is on that site.

---

## Era 4 — Stock recipe

**Goal:** after guest is up, server runs clone + stock compose/devcontainer.

| Deliver | Detail |
|---------|--------|
| **`wt-local`** | git clone; detect compose / `.devcontainer`; stock `compose up`; status phases / `last_error` |
| **`wt-api`** | ref, error, phase fields as needed |
| **CLI** | Pass source/ref; show errors only |

**Done when:** real repo comes up on `new`; failures are clean `Error`; `rm` cleans.

---

## Era 5 — Polish + seams

- Wait/poll UX for long provisions  
- known_hosts / keys for guests  
- Sync edge cases  
- Internal modules reusable by future `wt-control-plane` / `wt-worker` (don’t ship those bins until needed)  
- Short runbook: local workstation vs remote hypervisor  

---

## Suggested first commits

1. `wt-api` — minimal types + serde  
2. `wt-local` — helper + in-memory CRUD + stub create  
3. `wt-cli` — `bare_metal_local` + new/ls/rm + print  
4. `wt sync`  
5. `bare_metal_ssh` spawn  
6. libvirt  
7. recipe  

---

## Open (non-blocking)

- Exact helper argv (`wt-local api` vs flags)  
- Blocking create vs poll  
- Memory vs sqlite  
- Compose detection order (Era 4)  

## One-line summary

**Ship thin CLI + `wt-local` together on one machine first; SSH is just another way to run the same helper; then libvirt, then recipe.**
