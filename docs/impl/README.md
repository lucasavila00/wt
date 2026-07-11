# Implementation plan

Order of work. Product: [plan.md](../plan.md). Architecture: [arch/](../arch/README.md).  
Crates: `wt-api`, `wt-cli` (bin `wt`), `wt-local`.

## Approach

Ship a **vertical slice** of the product loop each era, then deepen the world implementation.

```text
Era 1:  wt ──SSH──► wt-local   (stub world, real API + print/sync Host)
Era 2:  real SSH guest         (libvirt + golden image)
Era 3:  stock recipe in guest  (clone + compose)
Era 4:  daily-driver UX
Era 5:  library seams for multi-node bins
```

Gesture each era protects:

```text
wt new <source> <name>  →  over SSH to site  →  printed guest Host snippet
wt ls / wt rm / wt sync
```

CLI design (SSH contexts, owner = SSH user, API, sync): [arch/cli.md](../arch/cli.md).  
Era 1: SSH context + CRUD over SSH + print; **`wt sync`** when list + guest endpoint exist.

---

## Era 1 — API + `wt-local` skeleton

| Layer | Delivers |
|-------|----------|
| **wt-api** | Create/list/get/delete types; `InstanceStatus`; serde JSON |
| **wt-local** | HTTP control plane; in-memory registry; stub embedded worker |
| **wt-cli** | SSH contexts (`ssh` + optional key); `new` / `ls` / `rm` **over SSH**; **print** + **`wt sync`** guest Hosts |

**Done when:** CLI SSHes to a box running `wt-local`; CRUD + sync work; types only in `wt-api`; no public API token required.

**Not in this era:** libvirt, compose, multi-node bins, public HTTPS control plane.

---

## Era 2 — Libvirt + SSH

| Layer | Delivers |
|-------|----------|
| **wt-local** | define/start/destroy guest; IP; SSH keys; status → `Running` |
| **wt-cli** | printed Host uses real endpoint |
| **ops** | golden image + network notes |

**Done when:** after applying the snippet, `ssh <name>` reaches the guest; `wt rm` destroys the domain.

---

## Era 3 — Stock recipe

| Layer | Delivers |
|-------|----------|
| **wt-local** | clone; detect compose/`.devcontainer`; stock `compose up` |
| **wt-api** / **wt-cli** | ref, errors, phases as needed |

**Done when:** real repo stacks come up on `new`; failures surface as `Error`; `rm` cleans up.

---

## Era 4 — Daily-driver UX

- Status polling / phases  
- Context/SSH key polish, known_hosts for guests  

- Sync/keys polish (sync itself is earlier)  
- Timeouts, clearer errors, landing shell polish  

---

## Era 5 — Seams for multi-node

- Control-plane and worker logic as libraries composed by `wt-local`  
- Stable control-plane HTTP contract; worker id fields for fleet list  
- Runbook  

**Not in this era:** shipping `wt-control-plane` / `wt-worker` or the k8s worker unless multi-node is actively needed.

---

## First coding slice (Era 1)

1. `wt-api` types  
2. `wt-local` listen + CRUD + stub worker  
3. `wt-cli` SSH contexts + client over SSH + print Host + `sync`  
4. Manual smoke

## Open (non-blocking)

- Blocking `new` vs async create + poll  
- Memory vs sqlite on `wt-local`  
- Compose detection details (Era 3)  

## One-line summary

**`wt-cli` + `wt-local` first: skeleton → SSH VM → recipe → UX → multi-node-ready libs.**
