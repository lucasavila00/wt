# Problem statement: `wt`

## Context

I already have a professional env contract:

- Docker images shared across local dev, CI, and e2e
- Devcontainers / Compose as the recipe for “what a working stack is”
- Real tests and parity with how the app runs in CI

What I do **not** have is a clean way to run **many parallel instances** of that recipe—one per feature/workstream—without inventing process every time.

Day-to-day cockpit is artisanal and works for one stream:

- byobu / tmux sessions
- ssh when needed
- docker / compose by hand
- git branches (sometimes worktrees, inconsistently)

That does not scale to “four trees, same image, different containers, different ports, different sessions.”

## The gap

| Solved | Missing |
|--------|---------|
| Image fidelity (dev ≈ CI ≈ e2e) | Multiplicity: N worktrees × N container sets |
| One-stack bring-up | Deterministic naming (project, ports, volumes) |
| Professional project layout | Lifecycle that matches how I actually work |

The industry is full of remote-env products (cloud boxes, agent workspaces, hosted devcontainers). Those solve a different problem: sellable compute and optional remote execution. I do not want to replace my cockpit or my image with a platform.

I want **instance management** for a recipe I already trust.

## Non-goals (for now)

- Hosted / SaaS remote environments
- Replacing Docker, Compose, or devcontainers
- Agent orchestration, PR automation, or full “task runtime” products
- Being the system of record for CI

Those may compose later. This tool is only the multiplicity layer.

## Desired solution: `wt`

A thin local CLI that turns “worktree + same image + unique container set + session” into a small lifecycle:

| Command | Meaning |
|---------|---------|
| `wt up <name>` | Create worktree + start env for that instance |
| `wt sh <name>` | Enter the session (cwd / shell / byobu for that instance) |
| `wt down <name>` | Stop containers; keep worktree and history |
| `wt rm <name>` | Stop + remove worktree (+ volumes optional) |
| `wt ls` | List instances: tree, ports, running? |

Mental model:

```text
same recipe (Compose / devcontainer / image I already use)
  × N named instances
      each: worktree + compose project + ports + session
```

Agents (or humans) work inside `wt sh <name>` so they never talk to the wrong tree’s containers.

## Success criteria

- Bringing up a second (or fifth) parallel stream is one command, not a checklist
- No port or container-name collisions between instances
- Same image/recipe as CI—no special “wt image”
- Fits existing habits (byobu, docker, git); does not demand a new IDE or cloud account
- Teardown is deliberate (`down` vs `rm`) so state isn’t lost by accident

## Decision process (docs path)

This repo should document decisions in order:

1. **Problem statement** (this file) — why, and what lifecycle we want  
2. **Architecture** (next) — shell vs other, how worktrees/compose/ports/sessions bind, what we refuse to own  
3. **Build** — implement `wt` against that architecture  

We discuss architecture only after this problem framing is agreed.

## One-line summary

> Named parallel instances of my existing Docker/devcontainer recipe—not a new remote-dev product.
