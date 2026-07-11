# Idealized API

Perfect **shape** of the product—not a full architecture. Mental map only.  
Context: [problem-statement.md](./problem-statement.md), [isolation-without-port-overrides.md](./isolation-without-port-overrides.md), [the-devcontainer-issue.md](./the-devcontainer-issue.md).

## The gesture

```text
$ wt new github.com:lucasavila00/frontend my-feature
# agent mints a world, recipe running, CLI writes ~/.ssh/config
ready  my-feature

$ ssh my-feature
# byobu on that world (feel: already inside the container)
```

Second stream = another name, another world/Host. Never another port in the app repo.

**Enter path is plain SSH.** `wt` gets you a world and a Host entry; daily attach is stock `ssh`.

## Overall arch

```text
Mac (CLI + stock ssh)
   │  wt …  (ensure instance exists / managed)
   │  maintains ~/.ssh/config  (name → that world)
   ▼
control plane / agent
   │  world + Docker + clone + stock compose
   ▼
one remote world per instance  ← SSH target for `ssh <name>`
```

| Layer | Job |
|-------|-----|
| **CLI** | Talk to agent; keep local SSH Host map; no Docker on Mac |
| **Agent** | Worlds that run the repo’s existing recipe |
| **ssh** | How you actually live on an instance |

Exact verb set, idempotency, and teardown rules: **later**. Same for provider under the agent (k8s, VMs, pool).

## Example commands

Illustrative only—not a locked lifecycle.

| Command | Meaning |
|---------|---------|
| `wt new <source> <name>` | Ensure instance exists; world + recipe; write SSH Host |
| `ssh <name>` | Enter (byobu / container feel) |
| `wt rm <name>` | Tear down instance; drop Host entry |
| `wt ls` | name, status, SSH target |

## What stays true

- Multiplicity = **worlds**, not port/project overrides in the app ([isolation](./isolation-without-port-overrides.md))
- One checkout per world keeps devcontainer-shaped recipes honest ([devcontainer issue](./the-devcontainer-issue.md))
- Recipe source = existing compose/devcontainer in the git repo; tool does not invent a new env format
- Session feel = byobu on the world, preferably already in the primary container

## Open questions (later)

1. Agent concrete form; warm pool vs cold.  
2. Lifecycle verbs and idempotency.  
3. SSH auth + how config is maintained (markers vs Include).  
4. How much of byobu/`docker exec` is RemoteCommand vs remote login.

## Lean (non-binding)

- Ensure a named instance from a repo → **SSH Host** → **`ssh <name>`**  
- Agent owns remote reality; CLI owns local map; recipe stays instance-blind  

## One-line summary

**`wt` leaves you with a Host in `~/.ssh/config`; `ssh <name>` is the product**—each name is its own remote world running stock compose, not a port fork of the app.
