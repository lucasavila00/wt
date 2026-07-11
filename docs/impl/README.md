# Implementation plan

Order of work given the current architecture.  
Product: [plan.md](../plan.md). Arch: [arch/](../arch/README.md). CLI design: [arch/cli.md](../arch/cli.md).

Crates: `wt-api`, `wt-local`, `wt-cli` (binary `wt`).

## Division of labor

| Piece | Responsibility |
|-------|----------------|
| **`wt-api`** | Shared request/response + status enums (serde JSON) |
| **`wt-local`** | **All real work**: owner-scoped instance state, stub→libvirt worker, recipe, GC. Invoked as **SSH remote command** helper (`JSON in → JSON out`). |
| **`wt-cli` (`wt`)** | **Thin client only**: read context config → build `ssh … -- <helper>` → exec → parse JSON → print tables/Host snippets / write managed ssh_config on `sync` |

```text
wt-cli:  config  →  ssh argv  →  exec  →  print (or sync files)
              │
              ▼
wt-local:  stdin JSON  →  do the thing  →  stdout JSON
```

Do **not** put provision/libvirt logic in the CLI. Do **not** start with a public HTTP server.

## Approach

Build **server-side truth first**, then a thin CLI, then deepen the worker.

```text
Era 1:  wt-api + wt-local helper (stub worlds)     ← start here
Era 2:  wt-cli (config + ssh exec + print/sync)
Era 3:  libvirt guests + real guest SSH endpoints
Era 4:  stock clone + compose/devcontainer in guest
Era 5:  daily UX polish + library seams for multi-node later
```

Manual test for Era 1 can run the helper **on the machine itself** (no Mac CLI yet):

```text
echo '{...}' | wt-local api   # or whatever the helper entrypoint is
```

Era 2 only wraps that with OpenSSH from another machine.

---

## Era 1 — `wt-api` + `wt-local` only

**Goal:** control-plane ops work on the site host with a stub worker.

| Deliver | Detail |
|---------|--------|
| **`wt-api`** | Create/list/get/delete payloads; `InstanceStatus`; guest `endpoint` fields; errors |
| **`wt-local`** | Binary/helper mode: read JSON request from stdin (or argv), write JSON response; in-memory registry; `owner` from process user / explicit field for local test; stub provision (no libvirt)—record name/source, optional fake endpoint |

**Done when:**

- On one Linux box, you can create / list / get / delete instances via the helper alone  
- Types live only in `wt-api`  
- No CLI required  

**Not in this era:** `wt-cli`, libvirt, compose, multi-node, public HTTP.

---

## Era 2 — Thin `wt-cli`

**Goal:** Mac (or laptop) drives Era 1 helper over SSH.

| Deliver | Detail |
|---------|--------|
| Context config | Sum type; only **`bare_metal_ssh`**: `name`, `kind`, `ssh`, optional `identity_file` / `port` |
| Commands | `context list\|use\|show`, `new`, `ls`, `rm`, `sync` (and optional `ssh` sugar) |
| Execution path | Read context → build OpenSSH command → `exec`/`output` → decode `wt-api` JSON → print result |
| **`sync`** | From list response, rewrite managed `~/.config/wt/ssh_config` for **my** instances with endpoints |
| Print | On `new`, print guest Host snippet when endpoint present |

CLI is **not** a second implementation of the API—only transport + UX.

**Done when:**

- From a client: `wt new/ls/rm/sync` against a host that has Era 1 `wt-local` installed  
- Owner = SSH user  
- No business logic beyond formatting and ssh_config projection  

**Not in this era:** real VMs (still stub endpoints unless you fake them server-side).

---

## Era 3 — Libvirt worlds

**Goal:** create path yields a real guest you can SSH into.

| Deliver | Detail |
|---------|--------|
| **`wt-local` only** | Golden image, define/start/destroy domain, wait for IP, inject keys, status → `Running` with real guest `endpoint` |
| **CLI** | Unchanged path; print/sync now useful for real Hosts |
| **Ops notes** | Image path, bridge/network, how to install helper on host |

**Done when:** `wt new` → `wt sync` → `ssh {repo}-{feature}` reaches the guest; `wt rm` destroys the domain.

**Not in this era:** full app compose (manual compose inside guest OK).

---

## Era 4 — Stock recipe

**Goal:** server runs clone + stock compose/devcontainer after the guest is up.

| Deliver | Detail |
|---------|--------|
| **`wt-local`** | git clone `source`[@ref]; detect compose / `.devcontainer`; stock `compose up`; phases / `last_error` |
| **`wt-api`** | Fields as needed for ref, error, status phases |
| **CLI** | Pass source/ref through; print errors—still no recipe logic |

**Done when:** real repo `new` brings stack up; failures are `Error` + message; `rm` still cleans.

---

## Era 5 — Polish + seams

| Area | Examples |
|------|----------|
| UX | Wait/poll UX for long provision; clearer errors; known_hosts; key paths |
| Sync | Edge cases, only-with-endpoint policy, multi-context later if needed |
| Structure | Internal modules so a future `wt-control-plane` / `wt-worker` can reuse logic; **do not** ship those bins unless multi-node is real |
| Runbook | Install `wt-local` on hypervisor, context example on laptop |

---

## Suggested first commits

1. **`wt-api`** — minimal instance types + serde  
2. **`wt-local`** — helper entrypoint, in-memory CRUD, stub create  
3. Smoke on one box without SSH  
4. **`wt-cli`** — parse `bare_metal_ssh` context, shell out to `ssh`, print JSON/table  
5. `sync` + Host printing  
6. Only then libvirt  

---

## Open (non-blocking)

- Exact helper argv (`wt-local api` vs dedicated binary name)  
- Blocking create vs async status poll  
- Memory vs sqlite for registry  
- Compose detection order (Era 4)  

## One-line summary

**Server first (`wt-local` + `wt-api`); CLI is config + `ssh` + print/sync; then libvirt; then recipe.**
