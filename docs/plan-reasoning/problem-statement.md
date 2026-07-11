# Problem statement

Product: **named parallel instances** of an existing Docker/devcontainer recipe, driven from a thin CLI over SSH. (Binary name TBD—not decided yet.)

Related notes: [isolation-without-port-overrides.md](./isolation-without-port-overrides.md), [the-devcontainer-issue.md](./the-devcontainer-issue.md), [idealized-api.md](./idealized-api.md), [bare-metal-worlds.md](./bare-metal-worlds.md).  
**Plan (decisions):** [plan.md](../plan.md).

## Who / setup

Author already has a professional env contract:

- Same Docker images for local-style dev, CI, and e2e
- Devcontainers and/or Compose define “what a working stack is”
- Real tests; parity with CI matters

Day-to-day cockpit:

- **Mac (or similar) is thin client only:** terminal, byobu/tmux client, editor
- **Containers never run on the Mac.** Always SSH to remote Linux for Docker/Compose/devcontainer
- Git checkout and compose live **on the remote**
- Today: one stream at a time, by hand (ssh, docker compose, byobu). Artisanal but works for N=1

## What’s missing

Not missing: image fidelity, a SaaS “cloud IDE,” or a new way to define the stack.

Missing: **multiplicity**—several features/workstreams in flight, each a full copy of the same recipe, without:

- a personal checklist every time
- tribal knowledge (“auth is on 3001, billing on 3002”)
- baking instance identity into the **application repo** (ports, public URLs, compose project names everywhere)

Industry remote-env products (hosted boxes, agent workspaces) sell compute and a different cockpit. Out of scope as a *requirement*. Remotes the author SSHes into (homelab, cloud VMs, etc.) are fine.

**Wanted:** instance management for a recipe already trusted.

## Isolation unit

| Avoid treating as the product | Actual unit |
|------------------------------|-------------|
| git worktree UX | **named instance** (usually ≈ branch name) |
| Mac/local Docker | **remote world** = SSH-reachable machine or VM where compose runs |
| Port remaps encoded in app config | Prefer **stock compose** per world ([isolation note](./isolation-without-port-overrides.md)) |

Preferred shape:

1. Instance name (e.g. branch or short slug)
2. One **remote world** for that instance (so ports/project names need not fork the recipe)—worlds come from a **trusted pool** (personal or shared team), not a secure multi-tenant product
3. Checkout on that remote (normal clone/directory; worktrees only if architecture later proves them worth it—not the identity of the tool)
4. Session = SSH (+ byobu) **on that world**—plain `ssh <name>` after the tool writes `~/.ssh/config` ([idealized API](./idealized-api.md))

Providers (home bare-metal vs company k8s): [plan.md](../plan.md).

## Devcontainer constraint

Typical devcontainer: **one host checkout bind-mounted** into the container → one filesystem identity. Fine for one world per remote. Painful if many instances share one Docker host and one tree. Details: [the-devcontainer-issue.md](./the-devcontainer-issue.md). Plan: one checkout per world; **same** `.devcontainer` + compose as today.

## Non-goals (for now)

- Running containers on the Mac / laptop Docker
- Requiring a hosted SaaS dev-environment product
- Replacing Docker, Compose, or the existing image/devcontainer **recipe** (no new GitLab-CI-like format for `wt`)
- Agent orchestration, PR automation, full “task runtime”
- Being CI system of record
- Being a git-worktree manager
- **Hostile multi-tenant isolation** — trusted pool; care about port/network identity only ([isolation](./isolation-without-port-overrides.md))
- Forcing k8s on a single personal server; requiring KubeVirt on every company cluster ([plan](../plan.md))

Multiplicity layer only. Other things may compose later.

## Desired lifecycle CLI

Illustrative—see [idealized-api.md](./idealized-api.md) for current gesture. Exact verbs later.

| Command | Meaning |
|---------|---------|
| `new <source> <name>` | World + stock recipe; write SSH Host |
| `ssh <name>` | Enter session (byobu on that world) |
| `rm <name>` | Tear down instance; drop Host |
| `ls` | List instances: name, SSH target, running? |

```text
same recipe (images + compose/devcontainer)
  × N named instances
      each: remote world + checkout + session
```

## Success criteria

- Second (or fifth) parallel stream is one command, not a checklist
- No port/name collisions between instances (via separate worlds—not by polluting the app with port matrices)
- Same images/recipe as existing devcontainer/compose; no special tool-only format
- Fits existing habits: byobu, ssh, docker on remote
- Framing stays **instances on remotes** (not git-worktree management)

## Docs / decision order

1. **Problem statement** (this file) — why, constraints  
2. **Isolation / devcontainer / bare-metal notes** — worlds stay stock  
3. **Idealized API** — gesture + SSH Host  
4. **Plan** — providers and build order ([plan.md](../plan.md))  
5. **Build**  

## One-line summary

Named parallel instances of an existing Docker/devcontainer recipe on **SSH remotes**; Mac is cockpit only—not Mac Docker, not a SaaS dev-env product.
