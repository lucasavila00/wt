# The devcontainer issue

Why default devcontainers fight **N parallel instances**, whether worktrees help, and reuse vs re-do.  
Cold-reader: [problem-statement.md](./problem-statement.md). Runtime: **Docker only on remote Linux**; Mac is SSH cockpit. “Host” = remote world, not laptop. Isolation: [isolation-without-port-overrides.md](./isolation-without-port-overrides.md). UX: [idealized-api.md](./idealized-api.md). **Plan:** [plan.md](./plan.md).

## Default model (“clone mirror”)

Usual local/remote-devcontainer behavior:

1. One git checkout on the host filesystem  
2. Tool **bind-mounts** that folder into the container (`workspaceMount` / automatic mount)  
3. Editor, shell, tools see that tree as `/workspaces/...` (or similar)  
4. Compose project / container naming often derives from that single workspace path  

Host and container share **one filesystem identity**—one tree, two views—not a second clone inside the container by default.

**Good for:** live edit, one feature at a time, “open folder → reopen in container.”  
**Bad for:** two branches with different files/`node_modules`/generated assets; two stacks from the same path without careful naming; anything that assumes workspace path **is** the instance.

**CI does not have this problem:** each job gets a fresh clone directory. Parallelism is natural. Devcontainer UX is optimized for **interactive single-workspace**, not fleet multiplicity.

## Why it bites this lifecycle

Desired:

```text
instance name → remote world → checkout + session → stock recipe
```

Devcontainer default:

```text
one host folder → one mount → one “current” workspace
```

**Plan fix:** one world per instance + **one clone inside that world**. Classic single-workspace devcontainer is fine **inside** the world. Pain returns only if many instances share one host tree + one Docker engine.

## Reuse vs re-do

| Path | Idea | Pros | Cons |
|------|------|------|------|
| **A. Reuse full devcontainer recipe** | Keep `.devcontainer/` + compose; tool only supplies worlds | One recipe for onboarding/IDE | Single-workspace-shaped—OK with one world per instance |
| **B. Reuse image + Compose only** | Runtime = compose in world; `.devcontainer` IDE companion | Close to CI images; thin tool | Keep knobs in sync if both paths used |
| **C. Re-do env contract** | New format (CI-like, custom) for k8s translation | Maps to pods easily | Throws away compose investment; dual-write tax |
| **D. Outsource runner** | DevPod, Codespaces, etc. | Their multi-workspace | Not small SSH/byobu control plane |

**Plan lean: A (canonical).** Exact same `.devcontainer` + Docker Compose as today. **Not C**—no new GitLab-CI-like format for `wt`. CI remains a separate batch file; shared contract is **images** (+ manual parity discipline).

## Can worktrees fix the mirror?

**Theory:** worktree = second directory + branch, shared object store.

**Practice (often fails):**

1. **`.git` is a file**, not a directory (`gitdir: …`). Mounting only the worktree breaks git in the container ([devcontainers/cli#796](https://github.com/devcontainers/cli/issues/796)).  
2. **UID / ownership** wars on bind mounts; second tree multiplies build dirs.  
3. **Product fit:** problem statement de-centers worktrees.

**Conclusion:** prefer **clone per world**, not worktrees. Spike worktrees only if disk dominates and gitdir+UID are explicit.

## Has the protocol evolved?

[Development Container Specification](https://containers.dev/) is still the open **recipe** format. Evolved toward multi-**service** (Compose), not multi-**instance** fleets. Industry added **runners**, not a fleet chapter of the spec.

**Plan:** keep recipe; multiplicity is **worlds + CLI** ([plan.md](./plan.md)), not waiting on the spec. Compose runs **inside** the world (VM or DinD pod)—not translated to native k8s Deployments as the primary path.

## Strategies

| # | Approach | Plan |
|---|----------|------|
| 1 | **Clone per world** | **Yes** — default |
| 2 | Worktree + gitdir mount | No, unless disk forces it |
| 3 | Clone into named volume | Optional later |
| 4 | Compose-first; devcontainer IDE-only | OK if runtime is compose-from-devcontainer config |
| 5 | Generated port/project overrides | Avoid; use separate worlds instead |

## Still open (detail)

- Runtime entry: full `devcontainer up` vs `docker compose` driven from the same config  
- How much of features/postCreate must run for interactive parity  

## Lean (non-binding)

- Keep **exact** image + Compose + `.devcontainer` recipe  
- Multiplicity = remote worlds + CLI, not a new format  
- Clone per world; worktrees not the identity of the tool  

## One-line summary

Devcontainer is single-workspace by design—reuse the same config, give each instance its own world and clone; don’t invent a parallel env language for k8s.
