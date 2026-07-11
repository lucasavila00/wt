# Implementation plan

How we build, in order. Product/arch: [plan.md](../plan.md), [arch/](../arch/README.md).  
Workspace: `crates/wt-api`, `wt`, `wt-agent`.

## Approach: thin end-to-end, then thicken

Full stack on day one (libvirt + golden image + clone + compose + SSH Host) is possible but **slow to first “it works”** and hard to debug.

**Preferred:** one **vertical slice** that exercises the real product gesture as early as possible, with **fake or minimal worlds**, then replace the fake with real layers one by one.

```text
Era 1:  CLI ↔ API ↔ agent  +  ssh config   (world = stub)
Era 2:  real SSH-able guest                (libvirt / golden image)
Era 3:  stock recipe in guest              (clone + compose)
Era 4:  status, errors, ops UX             (make it daily-driver)
Era 5:  harden + provider seam             (ready for a later k8s agent)
```

Gesture we protect every era:

```text
wt new <source> <name>  →  agent has instance  →  CLI prints SSH Host snippet
wt ls / wt rm
# later (stable path): CLI applies snippet to managed ssh config → ssh <name>
```

**SSH config auto-edit is not Era 1.** First only **print** the `Host` block (and any include instructions). Applying edits to `~/.ssh/config` (or an `Include`d file) is a **later smoothness feature**, once create/list/rm and real worlds are boringly reliable.

---

## Era 1 — Contract + loopback agent (E2E skeleton)

**Delivers:** you can run CLI against a local agent; instances exist in agent state; CLI **prints** the SSH config delta; shared types are real.

| Layer | What ships |
|-------|------------|
| **wt-api** | Create/list/get/delete types; `InstanceStatus` enum; serde JSON |
| **wt-agent** | HTTP server; in-memory (or sqlite) store; **stub provision** (no libvirt)—record name/source, fake or fixed endpoint optional |
| **wt** | `new` / `ls` / `rm`; config (agent URL); on `new`/`rm` **print** suggested `Host` add/remove (stdout)—**do not** edit `~/.ssh/config` yet |

**Done when:**

- `cargo run -p wt-agent` + `cargo run -p wt -- new …` creates a row  
- `wt ls` shows it; `wt rm` removes agent state and prints “remove this Host” guidance  
- `wt new` prints a pasteable `Host <name> …` block when an endpoint exists  
- Types only live in `wt-api` (no duplicate enums in CLI/agent)

**Explicitly not:** auto-editing ssh config, VMs, git clone, compose, byobu.

---

## Era 2 — Real world shell (libvirt + SSH)

**Delivers:** `ssh <name>` lands on a **real guest** the agent created.

| Layer | What ships |
|-------|------------|
| **wt-agent** | libvirt define/start/destroy from golden image; wait for IP; inject SSH key; status `Provisioning` → `Running` |
| **wt** | same commands; Host points at real guest IP/user |
| **ops** | documented golden image + bridge/network assumptions (single-dev) |

**Done when:**

- `wt new …` (source may still be ignored) → wait → `ssh <name>` works on empty Linux with Docker installed (or at least sshd)  
- `wt rm` destroys the domain  

**Explicitly not:** running the app recipe yet (or only optional manual compose inside).

---

## Era 3 — Stock recipe inside the world

**Delivers:** create path runs the **canonical** clone + compose/devcontainer flow.

| Layer | What ships |
|-------|------------|
| **wt-agent** | after SSH up: `git clone` source@ref; detect compose / `.devcontainer`; `docker compose up` (stock—no port rewrite) |
| **wt-api** | optional fields: ref, last_error detail, recipe phase in status |
| **wt** | pass source/ref through; surface recipe errors from agent |

**Done when:**

- `wt new <real-repo> <name>` → `ssh <name>` → stack from that repo’s compose is up (smoke repo OK)  
- Failure in clone/compose → `Error` + message; `rm` still cleans world  

---

## Era 4 — Daily-driver UX

**Delivers:** pleasant iteration without changing the architecture.

| Area | Examples |
|------|----------|
| Status | Poll/wait with phases (`Provisioning`, `StartingRecipe`, `Running`, `Error`) |
| Config | config.toml, tokens, known_hosts handling |
| **SSH config apply** | Only when path is stable: managed `Include` file / write Host on `new`, remove on `rm` (was print-only before) |
| Landing | byobu / default `docker exec` feel (if cheap) |
| Robustness | timeouts, destroy-on-failure policy, `ls` shows errors clearly |
| DX | progress output on `new`; refuse clobbering names |

**Done when:** single dev uses it for a real side project without hand-holding libvirt every time; auto Host edit is an opt-in or default only after print path proved correct.

---

## Era 5 — Harden + provider seam (not k8s yet)

**Delivers:** clean boundary so a future k8s agent is “another binary on `wt-api`,” not a rewrite.

| Layer | What ships |
|-------|------------|
| **wt-agent** | internal `WorldProvider` (or equivalent) trait: stub was Era 1, libvirt is Era 2–4 |
| **docs / API** | stable HTTP contract; version header or explicit compatibility note |
| **ops** | minimal runbook: agent install, image refresh, backup of state |

**Explicitly not this era:** implementing the k8s/DinD agent (see [arch/k8s-agent.md](../arch/k8s-agent.md)). Only ensure we did not paint ourselves into bare-metal-only types on the wire.

---

## Why this order (and why E2E-first still works)

| Risk if you invert | Mitigation here |
|--------------------|-----------------|
| Build libvirt for weeks with no CLI | Era 1 proves gesture + types first |
| CLI “done” against nothing real | Era 1 agent is real HTTP; Era 2 makes SSH real |
| Compose before SSH works | Era 2 then 3 |
| Abstract provider too early | Stub only as thin as needed; trait crystallizes Era 5 |

**Is pure E2E (all features) first possible?** Yes, but first success would take much longer. This plan is **E2E of the product loop** each era, with a growing definition of “world.”

```text
Era 1 E2E:  name exists in system + printed Host snippet
Era 2 E2E:  + real ssh (user may paste Host by hand)
Era 3 E2E:  + real stack
Era 4 E2E:  + not annoying (incl. optional auto ssh config edit when stable)
Era 5 E2E:  + not a dead-end for the next agent
```

---

## Suggested first coding slice (inside Era 1)

1. `wt-api`: `CreateInstanceRequest`, `Instance`, `InstanceStatus`, error body  
2. `wt-agent`: listen, in-memory map, CRUD  
3. `wt`: subcommands + HTTP client + **print** SSH Host snippet (no file writes)  
4. Manual: start agent, `new` / `ls` / `rm`, confirm printed config looks right  

No libvirt until Era 1 feels boring.

---

## Open (small; not blocking Era 1)

- Async create (202 + poll) vs blocking `new` until Running — pick simple for Era 1 (blocking or single-thread fake immediate Running)  
- sqlite vs memory for agent state — memory fine until Era 2  
- Exact compose detection order — Era 3  

## One-line summary

**Five eras: skeleton E2E → real SSH VM → stock recipe → daily UX → provider seam; never wait for k8s to learn if the product loop works.**
