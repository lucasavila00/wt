# CLI (`wt`)

Era 1/1.5 workstation CLI. Parent: [arch README](./README.md). Helper: [`wt-local`](../../crates/wt-local/).

## Responsibilities

| Does | Does not |
|------|----------|
| Spawn local `wt-local api` | Run libvirt or Docker itself |
| Send one JSON request over stdin | Use SSH |
| Parse one JSON response from stdout | Manage guest access |
| Create / list / remove my worlds | Manage Git or SSH credentials |

```text
wt  →  local wt-local api  →  wt-libvirt  →  KVM guest
```

Owner = local OS user running the helper.

## Context

Era 1 is local only. No client config. No context selection. `wt-local` resolves from `PATH`.

## Commands

| Command | Behavior |
|---------|----------|
| `wt new <source> <name> [--ref <ref>]` | Clone selected revision; start Compose; print name, status, IP |
| `wt ls` | List my worlds: name, status, IP |
| `wt rm <name>` | Destroy my world |

Era 1 keeps the implemented `wt new <name>` shape. Era 1.5 replaces it with the source/ref form above.

## Era 2

- Add local and OpenSSH context kinds.
- Select a named context before spawning the helper.
- Keep request/response behavior identical across transports.
- Do not add guest SSH or public HTTP.

```toml
current_context = "lab"

[[contexts]]
name = "lab"
kind = "bare_metal_ssh"
host = "wt-lab"
```

Remote invocation: `ssh -- wt-lab wt-local api`.

## One-line summary

**First run the real recipe locally; then carry the same helper API over OpenSSH.**
