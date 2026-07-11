# Problem statement

Product: **named parallel instances** of an existing Docker/devcontainer recipe, driven from a thin CLI over SSH.

Related: [isolation-without-port-overrides.md](./isolation-without-port-overrides.md), [the-devcontainer-issue.md](./the-devcontainer-issue.md), [idealized-api.md](./idealized-api.md), [bare-metal-worlds.md](./bare-metal-worlds.md).  
Plan: [../plan.md](../plan.md). Architecture: [../arch/README.md](../arch/README.md).

## Who / setup

Author already has a professional env contract:

- Same Docker images for local-style dev, CI, and e2e  
- Devcontainers and/or Compose define “what a working stack is”  
- Real tests; parity with CI matters  

Day-to-day cockpit:

- **Mac (or similar) is thin client only:** terminal, byobu/tmux client, editor  
- **Containers never run on the Mac.** Always SSH to remote Linux for Docker/Compose/devcontainer  
- Git checkout and compose live **on the remote**  
- Today without the tool: one stream at a time, by hand (ssh, docker compose, byobu)

## What’s missing

Not missing: image fidelity, a SaaS cloud IDE, or a new way to define the stack.

Missing: **multiplicity**—several features/workstreams in flight, each a full copy of the same recipe, without:

- a personal checklist every time  
- tribal knowledge (“auth is on 3001, billing on 3002”)  
- baking instance identity into the **application repo** (ports, public URLs, compose project names)

**Wanted:** instance management for a recipe already trusted.

## Isolation unit

| Avoid treating as the product | Actual unit |
|------------------------------|-------------|
| git worktree UX | **named instance** (usually ≈ branch name) |
| Mac/local Docker | **remote world** where compose runs |
| Port remaps in app config | **stock compose** per world ([isolation](./isolation-without-port-overrides.md)) |

Shape:

1. Instance name  
2. One **remote world** per instance (trusted pool)  
3. Checkout on that world  
4. Session via **`ssh <name>`** after Host config ([idealized API](./idealized-api.md)); site server is **`wt-local`** ([plan](../plan.md))

## Devcontainer constraint

One host checkout bind-mounted into the container → one filesystem identity. Fine with **one world per instance**. Painful if many instances share one Docker host and one tree. See [the-devcontainer-issue.md](./the-devcontainer-issue.md).

## Out of scope

- Docker on the Mac  
- Hosted SaaS dev-env product as a requirement  
- Replacing the existing image/devcontainer recipe  
- Full agent orchestration / PR automation / CI system of record  
- Git-worktree manager as the product  
- Hostile multi-tenant isolation  

## Success criteria

- Second (or fifth) parallel stream is one command, not a checklist  
- No port/name collisions via separate worlds—not app port matrices  
- Same images/recipe as existing devcontainer/compose  
- Fits byobu, ssh, docker on remote  
- Framing = instances on remotes, not worktree management  

## One-line summary

Named parallel instances of an existing Docker/devcontainer recipe on SSH remotes; Mac is cockpit only.
