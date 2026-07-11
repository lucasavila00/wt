# Isolation without port / project-name overrides

How to run **N copies** of the same Compose recipe **without** teaching the app about instance ports or `COMPOSE_PROJECT_NAME`.  
Cold-reader context: [problem-statement.md](./problem-statement.md). Devcontainer: [the-devcontainer-issue.md](./the-devcontainer-issue.md). Target UX: [idealized-api.md](./idealized-api.md). Bare metal: [bare-metal-worlds.md](./bare-metal-worlds.md). **Plan:** [plan.md](../plan.md).

## What “isolation” means here

**Care about:** each instance has its own **publish/port space** so stock `"3000:3000"` works N times—no app reconfig, no tribal port tables.

**Do not care about:** hostile multi-tenant security. Pool is **trusted**—one person, or a company team on a shared fleet. Coworkers (or your own other instances) are not treated as attackers.

| In scope | Out of scope |
|----------|----------------|
| Network / port multiplicity (own IP or host per world) | Sandboxing neighbors, gVisor, per-user network policy |
| Stable `ssh <name>` → right world | Assuming malicious access to the pool |
| Shared golden images, git creds, registry | Strong tenancy / blast-radius product |

So “one world per instance” is **L3/L4 identity for stock compose**, not a security boundary. Trust does not fix two stacks publishing `:3000` on the **same** host network—separate world/IP still required for that.

How worlds are supplied (libvirt vs k8s DinD pods): [plan.md](../plan.md).

## Hard constraint

- **Mac = cockpit only** (ssh, byobu, editor). **No Docker on Mac.**  
- Compose/devcontainer always on **remote Linux** (author SSHes in).  
- Design is **not** for Docker Desktop / Colima / OrbStack-on-Mac as the engine.  
- Pool may be personal or shared-company; **not** a multi-tenant SaaS trust model.

```text
Mac (no Docker) ──ssh──► remote Linux world(s) ──► docker compose
```

## Pain

If **many stacks share one Docker engine on one remote host network**:

| Collision | Common fix | Tax |
|-----------|------------|-----|
| Published ports (`"3000:3000"` twice) | Remap host ports per instance | Docs, e2e, OAuth redirects, muscle memory become instance-aware |
| Container/network/volume names | `COMPOSE_PROJECT_NAME=…` | Second identity on every command; easy to get wrong |
| Absolute URLs in app config | Per-instance env | App must know “I am instance B” |

**Goal:** same yml, same ports **inside** each stack, N copies. Multiplicity **outside** the application repo.

## Root cause

- **Inside** one Compose project, services already have isolated networking and talk via service DNS (`http://api:3000`). No remapping needed there.  
- Collision = **publishing** many stacks onto the **same host IP stack** (one remote, one Docker).  
- `COMPOSE_PROJECT_NAME` disambiguates object **names** on that daemon. It does **not** create a second host port 3000.

So: project name = naming isolation; port overrides = sharing one host network; **neither needed** if each stack has its own network identity (own machine, own VM/IP, or own pod netns) or never publishes to a shared host interface.

## Fix direction

Raise the isolation boundary. Identity = **which world**, not which remapped port.

```text
Mac ─ssh─► world-A  Docker  :3000  stock compose
Mac ─ssh─► world-B  Docker  :3000  stock compose
```

- Tool writes `~/.ssh/config` Host entries (`name →` world)  
- Repo stays instance-blind  
- Access: SSH/byobu primary; browser via tunnel, VPN, or private hostname—not a port matrix in git  

Contrast bad default: “feat-b is this box but port **3001** and project **proj-feat-b**.”

## Options (all remote)

| # | Setup | Repo awareness | Cost / notes |
|---|--------|----------------|--------------|
| **0** | One remote, shared Docker, port + project maps | **High** | Best hardware density; config poison—escape hatch, not target arch |
| **1** | One remote, shared Docker, **no host publish**; proxy/hostname or exec-only | Low–med | One box; still one kernel/daemon; browser needs naming layer |
| **2** | One remote as **hypervisor**; **KVM guest per instance** | Low | **Plan home path** ([bare-metal-worlds](./bare-metal-worlds.md), [plan](../plan.md)) |
| **3** | **One machine or cloud VM per instance** | **Lowest** | Simplest pure form; $ and fleet/pool |
| **4** | Strong container isolation (sysbox, etc.) without full VM | Low | Density; more exotic |
| **5** | **k8s Pod world + DinD** (compose inside pod) | Low | **Plan company path**; pod netns = GitLab-CI-like port free; needs DinD-friendly cluster |

**Lean (plan):** home **2** (KVM on bare metal); company **5** (k8s DinD worlds). Shared trusted pool either way. **0** only as density escape hatch that must **not** force app-repo port tables. Single personal server **via k8s only** = overkill when libvirt works.

**Anti-pattern:** treating “one Colima/Docker on a laptop” as isolation—wrong machine.  
**Anti-pattern:** designing for malicious neighbors—wrong threat model.  
**Anti-pattern:** inventing a new recipe format so k8s feels native—compose stays canonical ([plan](../plan.md)).

## What stays identical inside each world

If isolation is whole Docker host (machine, guest, or DinD-in-pod):

- `docker-compose.yml` published ports  
- Service DNS names  
- App defaults to `localhost:3000` **on that world**  
- CI images / Dockerfiles  
- e2e against localhost **inside** that world  

`COMPOSE_PROJECT_NAME` optional when only one stack runs on that engine.

## What the tool still owns (cannot vanish)

| Concern | Lives in |
|---------|----------|
| name → SSH target | CLI `~/.ssh/config` Host entries |
| provision / destroy world | **Provider:** bare-metal agent (libvirt) or k8s agent (DinD pods) |
| checkout on remote | clone/fetch inside world |
| compose up | inside world, stock recipe |
| browser reachability | optional tunnel/DNS—not compose edits in git |

## Fit with other constraints

| Piece | Implication |
|-------|-------------|
| Devcontainer | “Host” for bind-mount = remote world; **one workspace per world**. |
| byobu / ssh | `ssh <name>` on that world—natural. |
| CI | Same **images**; CI job wiring stays separate from interactive compose. |
| Mac bind mount | Sidestepped: checkout lives on remote inside the world. |

## Risks / lies to avoid

1. VMs mean zero ops—still need golden image + agent  
2. Shared daemon + clever ports = “temporary” that leaks into the repo  
3. Encoding instance ports in the application  
4. Assuming every company cluster allows DinD—need a **dev** cluster/pool that does  
5. Running home solo box through full k8s “because company uses k8s”

## Still open (detail, not direction)

- Warm pool vs cold provision sizing  
- Browser: tunnel vs private DNS  
- SSH auth mechanics (keys vs certs; Include vs markers in ssh config)

## Lean (non-binding)

- Isolation = **no port soup**, not sandbox the coworker  
- One **world** per instance (VM or DinD pod) in a **trusted pool**  
- Mac = SSH + CLI only; two providers per [plan.md](../plan.md)  
- Shared-daemon overrides = density escape hatch, not what the repo is written for  

## One-line summary

Never run Docker on the Mac—only inside remote worlds; avoid port soup with one world per instance (trusted pool); CLI maps names to SSH targets, not remapped ports in the app.
