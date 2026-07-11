# Implementation plan

Order of work. Product: [plan.md](../plan.md). Architecture: [arch/](../arch/README.md).  
Crates: `wt-api`, `wt`, `wt-local`.

## Approach

Ship a **vertical slice** of the product loop each era, then deepen the world implementation.

```text
Era 1:  wt ↔ wt-local          (stub world, real API + print Host)
Era 2:  real SSH guest         (libvirt + golden image)
Era 3:  stock recipe in guest  (clone + compose)
Era 4:  daily-driver UX
Era 5:  library seams for multi-node bins
```

Gesture each era protects:

```text
wt new <source> <name>  →  instance on control plane  →  printed SSH Host snippet
wt ls / wt rm
```

SSH config **file** edits are Era 4 polish. Eras 1–3 print only.

---

## Era 1 — API + `wt-local` skeleton

| Layer | Delivers |
|-------|----------|
| **wt-api** | Create/list/get/delete types; `InstanceStatus`; serde JSON |
| **wt-local** | HTTP control plane; in-memory registry; stub embedded worker |
| **wt** | `new` / `ls` / `rm`; control-plane URL; **print** Host snippets |

**Done when:** `wt-local` + `wt new/ls/rm` work end-to-end; types live only in `wt-api`.

**Not in this era:** libvirt, compose, multi-node bins, auto ssh config.

---

## Era 2 — Libvirt + SSH

| Layer | Delivers |
|-------|----------|
| **wt-local** | define/start/destroy guest; IP; SSH keys; status → `Running` |
| **wt** | printed Host uses real endpoint |
| **ops** | golden image + network notes |

**Done when:** after applying the snippet, `ssh <name>` reaches the guest; `wt rm` destroys the domain.

---

## Era 3 — Stock recipe

| Layer | Delivers |
|-------|----------|
| **wt-local** | clone; detect compose/`.devcontainer`; stock `compose up` |
| **wt-api** / **wt** | ref, errors, phases as needed |

**Done when:** real repo stacks come up on `new`; failures surface as `Error`; `rm` cleans up.

---

## Era 4 — Daily-driver UX

- Status polling / phases  
- config.toml, tokens, known_hosts  
- Optional **apply** of SSH config when the print path is trusted  
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
3. `wt` client + print Host  
4. Manual smoke  

## Open (non-blocking)

- Blocking `new` vs async create + poll  
- Memory vs sqlite on `wt-local`  
- Compose detection details (Era 3)  

## One-line summary

**`wt` + `wt-local` first: skeleton → SSH VM → recipe → UX → multi-node-ready libs.**
