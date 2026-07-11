# Idealized API

Perfect **shape** of the product—not a full architecture. Mental map only.  
Context: [problem-statement.md](./problem-statement.md), [isolation-without-port-overrides.md](./isolation-without-port-overrides.md), [the-devcontainer-issue.md](./the-devcontainer-issue.md), [bare-metal-worlds.md](./bare-metal-worlds.md). **Plan:** [plan.md](./plan.md).

## The gesture

```text
$ wt new github.com:lucasavila00/frontend my-feature
# agent mints a world, stock devcontainer/compose up, CLI writes ~/.ssh/config
ready  my-feature

$ ssh my-feature
# byobu on that world (feel: already inside the container)
```

Second stream = another name, another world/Host. Never another port in the app repo.

**Enter path is plain SSH.** `wt` gets you a world and a Host entry; daily attach is stock `ssh`.

## Overall arch

```text
Mac (CLI + stock ssh)
   │  wt …  maintains ~/.ssh/config  (name → that world)
   ▼
agent API
   │
   ├─ bare-metal provider (libvirt VMs)     ← home / 1–2 servers
   └─ k8s provider (DinD pod worlds)      ← company dev cluster
   ▼
world: Docker + clone + stock .devcontainer/compose
```

| Layer | Job |
|-------|-----|
| **CLI** | Talk to agent; keep local SSH Host map; no Docker on Mac |
| **Agent + provider** | Create/destroy worlds ([plan.md](./plan.md)) |
| **ssh** | How you live on an instance |

Exact verb set and idempotency: later. Provider choice is **not** later—see plan.

## Example commands

Illustrative only—not a locked lifecycle.

| Command | Meaning |
|---------|---------|
| `wt new <source> <name>` | Ensure instance exists; world + recipe; write SSH Host |
| `ssh <name>` | Enter (byobu / container feel) |
| `wt rm <name>` | Tear down instance; drop Host entry |
| `wt ls` | name, status, SSH target |

## What stays true

- Multiplicity = **worlds**, not port overrides in the app ([isolation](./isolation-without-port-overrides.md))
- **Trusted pool**; isolation = stock ports N times, not multi-tenant security  
- Recipe = **exact same** `.devcontainer` + compose—no new format ([plan](./plan.md))  
- One checkout per world ([devcontainer issue](./the-devcontainer-issue.md))  
- Session feel = byobu on the world, preferably already in the primary container  
- Two providers (bare-metal + k8s); not “k8s everywhere including one home box”

## Still open (detail)

- Lifecycle verbs beyond the gesture; idempotency  
- SSH auth + ssh config maintenance (markers vs Include)  
- How much of byobu/`docker exec` is RemoteCommand vs remote login  

## Lean (non-binding)

- **`wt new` → SSH Host → `ssh <name>`**  
- Agent/providers own remote reality; CLI owns local map; recipe instance-blind  

## One-line summary

**`wt` leaves you with a Host in `~/.ssh/config`; `ssh <name>` is the product**—each name is its own world running stock compose, via bare-metal or k8s provider.
