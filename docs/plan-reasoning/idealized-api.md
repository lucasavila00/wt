# User workflow

Implemented product workflow. Plan: [../plan.md](../plan.md). Architecture:
[../arch/](../arch/README.md).

## Gesture

```text
$ wt new git@github.com:lucasavila00/frontend.git lab.frontend-my-feature
# CLI invokes the configured server; creates world; prints guest aliases
$ wt sync
$ ssh frontend-my-feature
# VS Code Remote SSH target: frontend-my-feature
```

Second stream = another `{repo}-{feature}`, another world. Never another port in the app repo.  
Full CLI: [../arch/cli.md](../arch/cli.md).

## Overall arch

```text
Client (wt + stock OpenSSH)
   │  wt → bare_metal_ssh | bare_metal_local
   ▼
wt-server (via ssh -- helper, or local helper)
   │
   ▼
guest world: Docker + clone + stock compose
   ▲
   └── ssh Host after sync
```

| Layer | Job |
|-------|-----|
| **CLI** | Local and SSH contexts; owner-scoped API over stdio; print and sync guest aliases |
| **Control plane + worker** | Worlds and inventory ([architecture](../arch/README.md)) |
| **ssh** | Server hop (API) and world hop (guest) |

Server SSH transports the helper API. Guest SSH enters a provisioned world. They
are separate connections with separate authentication and host identities.

## Example commands

| Command | Meaning |
|---------|---------|
| `wt new <source> <name>` | Create; print SSH Host snippet (`name` = `{repo}-{feature}`) |
| `wt sync` | Rewrite managed ssh config from **my** instances on this cluster |
| `ssh <name>` | Enter |
| `wt rm <name>` | Tear down |
| `wt ls` | name, status, SSH target |
| qualified `context.world` name | Select a specific configured server |

## What stays true

- Multiplicity = **worlds**, not port overrides in the app  
- Trusted pool  
- Recipe = existing `.devcontainer` + compose  
- One checkout per world  
- Session feel = byobu on the world, preferably in the primary container  

## One-line summary

**`wt` + control plane leave you able to `ssh <name>` into a stock-compose world.**
