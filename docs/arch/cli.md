# CLI (`wt`)

Era 1 workstation CLI. Parent: [arch README](./README.md). Helper: [`wt-local`](../../crates/wt-local/).

## Responsibilities

| Does | Does not |
|------|----------|
| Spawn local `wt-local api` | Run libvirt or Docker itself |
| Send one JSON request over stdin | Use SSH |
| Parse one JSON response from stdout | Manage guest access |
| Create / list / remove my worlds | Run repository recipes |

```text
wt  →  local wt-local api  →  wt-libvirt  →  KVM guest
```

Owner = local OS user running the helper.

## Context

Era 1 has one context kind:

```toml
[[contexts]]
name = "local"
kind = "bare_metal_local"
# helper = "wt-local"
# helper_args = ["api"]
```

No config file means one implicit local context. Config path: `~/.config/wt/config.toml`.

## Commands

| Command | Behavior |
|---------|----------|
| `wt new <source> <name>` | Create KVM world; print name, status, IP |
| `wt ls` | List my worlds: name, status, IP |
| `wt rm <name>` | Destroy my world |

`source` is stored but not cloned in Era 1.

## Later

- Remote helper transport
- Guest access
- Context management commands
- Recipe execution

## One-line summary

**Era 1 `wt` is a thin local stdio client for `wt-local`.**
