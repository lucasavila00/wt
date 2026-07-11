# Isolation without port / project-name overrides

How to run **N copies** of the same Compose recipe without teaching the app about instance ports or `COMPOSE_PROJECT_NAME`.  
Plan: [../plan.md](../plan.md). Worlds on bare metal: [bare-metal-worlds.md](./bare-metal-worlds.md).

## What “isolation” means

**In scope:** each instance has its own **publish/port space** so stock `"3000:3000"` works N times.

**Out of scope:** hostile multi-tenant security. Pool is **trusted** (solo or same company).

| In scope | Out of scope |
|----------|----------------|
| Network / port multiplicity (own IP or host per world) | Sandboxing neighbors |
| Stable `ssh <name>` → right world | Malicious pool members as a design threat |
| Shared golden images, git creds, registry | Strong tenancy product |

Trust does not fix two stacks publishing `:3000` on the **same** host network—separate world/IP (or pod netns) is still required.

## Hard constraints

- Mac = cockpit only; **no Docker on Mac**  
- Compose/devcontainer on **remote Linux**  
- Worlds from a trusted pool (personal or shared company)

```text
Mac (no Docker) ──ssh──► remote world(s) ──► docker compose
```

## Pain (shared Docker host network)

| Collision | Common fix | Tax |
|-----------|------------|-----|
| Published ports twice | Remap host ports | Docs, e2e, OAuth, muscle memory become instance-aware |
| Object names | `COMPOSE_PROJECT_NAME` | Second identity on every command |
| Absolute URLs | Per-instance env | App knows “I am instance B” |

**Goal:** same yml, same ports **inside** each stack; multiplicity outside the app repo.

## Fix

Identity = **which world**, not which remapped port.

```text
Mac ─ssh─► world-A  Docker  :3000  stock compose
Mac ─ssh─► world-B  Docker  :3000  stock compose
```

Tool maps names to SSH endpoints (print Host and apply with `wt sync` in Era 1.5). Repo stays instance-blind.

## How worlds are supplied

| Setup | Notes |
|-------|--------|
| KVM guest per instance on bare metal | Home path; **`wt-local`** ([arch](../arch/bare-metal-agent.md)) |
| k8s Pod + DinD | Company path when cluster allows; own netns |
| Shared daemon + port maps | Escape hatch only; must not force app-repo port tables |

## What stays identical inside each world

- Published ports in compose  
- Service DNS  
- `localhost:3000` **on that world**  
- CI images / Dockerfiles  

## What the tool owns

| Concern | Lives in |
|---------|----------|
| name → guest SSH target | CLI print + `wt sync` managed ssh config |
| provision / destroy world | CLI over SSH to `wt-local` (control plane + worker) |
| checkout + compose | inside world |

## One-line summary

**One remote world per instance with stock compose; CLI maps names to SSH targets, not remapped ports in the app.**
