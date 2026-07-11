# Isolation without port / project-name overrides

How to run **N copies** of the same Compose recipe **without** teaching the app about instance ports or `COMPOSE_PROJECT_NAME`.  
Cold-reader context: [problem-statement.md](./problem-statement.md). Devcontainer single-workspace constraint: [the-devcontainer-issue.md](./the-devcontainer-issue.md).

## Hard constraint

- **Mac = cockpit only** (ssh, byobu, editor). **No Docker on Mac.**  
- Compose/devcontainer always on **remote Linux** (author SSHes in).  
- Design is **not** for Docker Desktop / Colima / OrbStack-on-Mac as the engine.

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

So: project name = naming isolation; port overrides = sharing one host network; **neither needed** if each stack has its own network identity (own machine, own VM/IP) or never publishes to a shared host interface.

## Fix direction

Raise the isolation boundary. Identity = **which world**, not which remapped port.

```text
Mac ─ssh─► world-A  Docker  :3000  stock compose
Mac ─ssh─► world-B  Docker  :3000  stock compose
```

- Tool (and SSH config) maps `name → user@host`  
- Repo stays instance-blind  
- Access: SSH/byobu primary; browser via tunnel, VPN, or private hostname—not a port matrix in git  

Contrast bad default: “feat-b is this box but port **3001** and project **proj-feat-b**.”

## Options (all remote)

| # | Setup | Repo awareness | Cost / notes |
|---|--------|----------------|--------------|
| **0** | One remote, shared Docker, port + project maps | **High** | Best hardware density; config poison—escape hatch, not target arch |
| **1** | One remote, shared Docker, **no host publish**; proxy/hostname or exec-only | Low–med | One box; still one kernel/daemon; browser needs naming layer |
| **2** | One remote as **hypervisor**; **KVM (or similar) guest per instance** | Low | Own IP + Docker per guest; stock compose; ops = libvirt/images/DHCP |
| **3** | **One machine or cloud VM per instance** | **Lowest** | Simplest pure form; $ and fleet/pool; warm images mitigate latency |
| **4** | Strong container isolation (sysbox, etc.) without full VM | Low | Density; more exotic |

**Lean:** **3**, or **2** when one physical box must densify. **0** allowed only as density mode that must **not** force app-repo port tables.

**Anti-pattern:** treating “one Colima/Docker on a laptop” as isolation—wrong machine; author doesn’t run Docker there anyway. One Docker host = one publish port space.

## What stays identical inside each world

If isolation is whole Docker host (machine or guest):

- `docker-compose.yml` published ports  
- Service DNS names  
- App defaults to `localhost:3000` **on that remote**  
- CI images / Dockerfiles  
- e2e against localhost **inside** that world  

`COMPOSE_PROJECT_NAME` optional/unnecessary when only one stack runs on that engine (directory default is enough).

## What the tool still owns (cannot vanish)

Multiplicity always has an identity **somewhere**. Prefer tool/infra, not app:

| Concern | Lives in |
|---------|----------|
| name → SSH target | CLI state; optional generated `~/.ssh/config` Host entries |
| provision / start / stop / destroy world | Provider: static inventory, cloud API, or libvirt on a hypervising remote |
| checkout on remote | clone/fetch inside world |
| compose up/down | remote commands over SSH |
| browser reachability | optional `ssh -L`, private DNS, one-at-a-time forward—not compose edits in git |

## Fit with other constraints

| Piece | Implication |
|-------|-------------|
| Devcontainer | “Host” for bind-mount = remote world; **one workspace per world** matches the spec. Avoid many instances on one bind-mounted tree. |
| byobu / ssh | `sh <name>` = SSH + session on that world—natural. |
| CI | Each world ≈ long-lived CI-shaped machine; stock images. |
| Devcontainer “Mac bind mount” problems | Largely sidestepped: checkout lives on remote inside the world. |

## Risks / lies to avoid

1. VMs mean zero ops—still need golden image + control plane  
2. Shared daemon + clever ports = “temporary” that leaks into the repo  
3. Encoding instance ports in the application so remotes stay “flexible”  
4. Classical bare-metal KVM language on Mac cockpit—irrelevant; KVM/libvirt matter **on the remote Linux** hypervising host  

## Open questions (architecture)

1. Fleet model: static SSH pool vs on-demand cloud VMs vs libvirt guests on one bare metal?  
2. Warm pool (preinstalled Docker/images) vs cold provision?  
3. Who owns DNS/SSH naming (only the CLI vs Tailscale/homelab)?  
4. Browser: dynamic single tunnel vs stable per-instance hostnames on private net?  
5. Is shared-daemon+overrides a supported “cheap mode” or forbidden for docs/app?  
6. Where CLI state lives (Mac-local map name→host vs remote registry)?  

## Lean (non-binding)

- Port/project sprawl is an artifact of **sharing one host network/Docker**  
- Keep compose + app instance-blind via **one remote world per instance** (machine or VM)  
- Mac = SSH control plane only  
- Shared-daemon overrides = density escape hatch, not what the repo is written for  

## One-line summary

Never run Docker on the Mac—only on SSH remotes; avoid port/project soup by giving each instance its own remote world with stock compose; the CLI maps names to SSH targets, not remapped ports in the app.
