# The devcontainer issue

Why default devcontainers fight **N parallel instances**, whether worktrees help, whether the spec evolved, and reuse vs re-do.  
Cold-reader context: see [problem-statement.md](./problem-statement.md). Runtime: **Docker only on remote Linux**; Mac is SSH cockpit. “Host” below = that remote (or a VM on it), not the laptop. Isolation preference (one remote world per instance): [isolation-without-port-overrides.md](./isolation-without-port-overrides.md). Target UX: [idealized-api.md](./idealized-api.md).

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
instance name → remote world → checkout + session → stock (or nearly stock) recipe
```

Devcontainer default:

```text
one host folder → one mount → one “current” workspace
```

Until each instance has a clear filesystem root (and, if sharing a Docker daemon, unambiguous project/port identity), `up` / `sh` / `down` cannot be honest.

**Overrides** (`devcontainer` merge, `compose` override, env files) can fix **names and ports**. They do **not** create two git trees by themselves—you need two directories, a volume clone, or similar.

If architecture adopts **one remote world (machine/VM) per instance** with one checkout inside it, classic single-workspace devcontainer is fine **inside that world**. Pain returns only when many instances share one host + one tree + one Docker engine.

## Reuse vs re-do

| Path | Idea | Pros | Cons |
|------|------|------|------|
| **A. Reuse full devcontainer recipe** | Keep `.devcontainer/`; CLI drives instances (overrides, which folder, which host) | One recipe for onboarding/IDE; features, postCreate, editor metadata stay | Still single-workspace-shaped; multi-root/multi-dir care |
| **B. Reuse image + Compose only** | Runtime path = `docker compose` over SSH; `.devcontainer` optional for IDE | Close to CI; thin CLI; matches byobu/ssh cockpit | Possible drift IDE vs CLI (mitigate: one source of knobs) |
| **C. Re-do env contract** | New format only (nix, custom, etc.) | Full control | Throws away investment; team friction |
| **D. Outsource runner** | DevPod, Codespaces, etc. still fed by `devcontainer.json` | Multi-workspace as their product | Extra tool; may not match ssh/byobu; not “small local control plane” |

**Lean:** **A or B**, not C. Real asset is **image + Compose (+ scripts)**. Question is how much **devcontainer tooling** (CLI, mount semantics, features) is runtime vs IDE companion. Do not re-author the stack unless A/B fail after a spike.

## Can worktrees fix the mirror?

**Theory:** worktree = second directory + branch, shared object store. Mount each worktree path as its own workspace → two mirrors, two stacks, no full second object clone.

**Practice (often fails):**

1. **`.git` is a file**, not a directory (`gitdir: /path/to/main/.git/worktrees/...`). Mounting only the worktree folder omits the real git dir → git inside container breaks (not a repo, bad commits, submodules). Long-standing; [devcontainers/cli#796](https://github.com/devcontainers/cli/issues/796) still tracks first-class worktree support (also mount `gitdir`).  
2. **UID / ownership:** bind mounts keep host UIDs; container user (`vscode`/root) vs host → dubious ownership, root-owned files, chown wars. Second tree multiplies `node_modules`/build dirs. Author has hit this before.  
3. **Product fit:** problem statement de-centers worktrees. Using them only as a mount trick still couples to git worktree layout and tool edge cases.

**Conclusion:** worktrees can supply a second **path**; they do **not** fix the model alone. Default assumption: **prefer separate checkouts (or one world per instance with one clone inside)** over worktrees. Spike worktrees only if disk dominates **and** gitdir mount + UID policy are explicit.

## Has the protocol evolved? Have others superseded it?

[Development Container Specification](https://containers.dev/) (`devcontainer.json`) is still the open **recipe** format: image/Dockerfile/Compose, features, lifecycle scripts, mounts, ports, user. Supported by VS Code, Codespaces, JetBrains (partial), DevPod, Ona/Gitpod, etc.

**Evolved toward:** multi-**service** (Compose), features marketplace, named volume / “clone into volume,” `workspaceMount` / `workspaceFolder` overrides.

**Did not become:** multi-**instance** protocol (N named isolated copies of the same recipe). No first-class fleet. Worktree support incomplete. VS Code: one container attach per window (more windows if needed).

So: fair that “the protocol should evolve” for parallel instances; industry papered over it with **runners/products**, not a multi-instance chapter of the spec.

| Layer | Status |
|-------|--------|
| Spec | Still common interchange; not dead |
| Runners | Implement/supersede *execution*; don’t obsolete the recipe file |
| Compose-only runtime | Peer to “open as devcontainer”; fine for SSH cockpit |
| VM/machine per workspace | Sidesteps shared bind-mount identity (aligns with isolation note) |

**Do not abandon** image + Compose recipe lightly. **May sideline** “reopen folder as devcontainer” as the only run path if compose-over-SSH is enough for daily use. Keep `.devcontainer/` for IDE if useful; generate any per-instance knobs from the same control plane as the CLI.

## Strategies (architecture picks)

| # | Approach | Pros | Cons |
|---|----------|------|------|
| 1 | **Clone per world** on remote | CI-like; fewest git special cases | Disk; clone time (ref/partial clone mitigations) |
| 2 | Worktree + explicit **gitdir** mount | Saves objects | Fragile; ownership; tooling gaps |
| 3 | Clone into **named volume** | Less host bind pain; documented pattern | Host editor story harder unless extra mounts |
| 4 | **Compose-first**; devcontainer IDE-only | Matches ssh/byobu; thin tool | Two entrypaths—keep in sync |
| 5 | Generated **overrides** (project name, ports) | Needed if many stacks share one Docker host | Reintroduces instance-awareness unless only tool-local |

With isolation lean (**one machine/VM per instance**): problem shrinks to “one checkout inside world; stock single-workspace devcontainer OK.” Strategies 2–5 matter mainly for shared-daemon density mode.

## Open questions for architecture

1. Runtime: full `devcontainer up` over SSH vs compose-only (+ optional IDE)?  
2. Filesystem: multi-clone vs worktree+gitdir vs volume? (Default lean: clone per world.)  
3. UID policy if bind mounts remain  
4. How sacred is `.devcontainer` (features, postCreate) vs already in the CI image?  
5. Minimal repo contract (if any) so the tool stays thin  

## Lean (non-binding)

- Pain is real; worktrees are not a free fix  
- Reuse **image + Compose**; treat full devcontainer as recipe/IDE companion, not the multiplicity engine  
- Multiplicity lives in **remote worlds + CLI** ([isolation](./isolation-without-port-overrides.md)), not in waiting for the spec  

## One-line summary

Devcontainer bind-mount = single-workspace design; keep the recipe, don’t expect multi-instance from the protocol—separate remote worlds (and clones) do multiplicity; worktrees need gitdir+UID discipline if used at all.
