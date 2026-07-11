# Idealized API

Target product shape. Plan: [../plan.md](../plan.md). Arch: [../arch/](../arch/README.md).

## Gesture

```text
$ wt new github.com:lucasavila00/frontend frontend-my-feature
# CLI SSHes to site (context); creates world; prints guest Host snippet
$ wt sync
$ ssh frontend-my-feature
```

Second stream = another `{repo}-{feature}`, another world. Never another port in the app repo.  
Full CLI: [../arch/cli.md](../arch/cli.md).

## Overall arch

```text
Mac (CLI + stock OpenSSH)
   │  wt → bare_metal_ssh | bare_metal_local
   ▼
wt-local (via ssh -- helper, or local helper)
   │
   ▼
guest world: Docker + clone + stock compose
   ▲
   └── ssh Host after sync
```

| Layer | Job |
|-------|-----|
| **CLI** | SSH contexts; owner-scoped API over SSH; print + `sync` guest Hosts |
| **Control plane + worker** | Worlds and inventory ([control-plane](../arch/control-plane.md)) |
| **ssh** | Site hop (API) and world hop (guest) |

## Example commands

| Command | Meaning |
|---------|---------|
| `wt new <source> <name>` | Create; print SSH Host snippet (`name` = `{repo}-{feature}`) |
| `wt sync` | Rewrite managed ssh config from **my** instances on this cluster |
| `ssh <name>` / `wt ssh <name>` | Enter |
| `wt rm <name>` | Tear down |
| `wt ls` | name, status, SSH target |
| `wt context …` | Which cluster |

## What stays true

- Multiplicity = **worlds**, not port overrides in the app  
- Trusted pool  
- Recipe = existing `.devcontainer` + compose  
- One checkout per world  
- Session feel = byobu on the world, preferably in the primary container  

## One-line summary

**`wt` + control plane leave you able to `ssh <name>` into a stock-compose world.**
