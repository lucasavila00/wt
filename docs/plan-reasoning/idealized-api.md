# Idealized API

Target product shape. Plan: [../plan.md](../plan.md). Arch: [../arch/](../arch/README.md).

## Gesture

```text
$ wt new github.com:lucasavila00/frontend my-feature
# control plane creates world + recipe; CLI prints SSH Host snippet
ready  my-feature

# user applies Host (or later: tool applies when that UX exists)
$ ssh my-feature
# byobu on that world (feel: already inside the container)
```

Second stream = another name, another world. Never another port in the app repo.

## Overall arch

```text
Mac (CLI + stock ssh)
   │  wt → control-plane URL
   ▼
wt-local  (or later wt-control-plane + workers)
   │
   ▼
world: Docker + clone + stock .devcontainer/compose
```

| Layer | Job |
|-------|-----|
| **CLI** | Control-plane client; print (optionally later apply) Host map |
| **Control plane + worker** | Worlds and inventory ([control-plane](../arch/control-plane.md)) |
| **ssh** | How you live on an instance |

## Example commands

| Command | Meaning |
|---------|---------|
| `wt new <source> <name>` | Ensure instance; print SSH Host snippet |
| `ssh <name>` | Enter (after Host is configured) |
| `wt rm <name>` | Tear down; print Host removal guidance |
| `wt ls` | name, status, SSH target |

## What stays true

- Multiplicity = **worlds**, not port overrides in the app  
- Trusted pool  
- Recipe = existing `.devcontainer` + compose  
- One checkout per world  
- Session feel = byobu on the world, preferably in the primary container  

## One-line summary

**`wt` + control plane leave you able to `ssh <name>` into a stock-compose world.**
