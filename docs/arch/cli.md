# CLI (`wt`)

Mac (or any cockpit) binary. No Docker.  
Parent: [arch README](./README.md). Control plane: [control-plane.md](./control-plane.md). Server: [bare-metal-agent.md](./bare-metal-agent.md).

## Responsibilities

| Does | Does not |
|------|----------|
| Call control-plane API (create / list / destroy) | Run compose, libvirt, or clones |
| **Print** SSH `Host` snippets for the user to apply | Own long-term instance inventory |
| Show status / errors from the control plane | Know libvirt or k8s details |
| Config: control-plane URL (+ token) | Talk to workers directly |

Automatic editing of `~/.ssh/config` (or a managed `Include` file) is optional later UX; the supported path is print → user applies → `ssh <name>`.

## Commands

| Command | Behavior |
|---------|----------|
| `wt new <source> <name>` | Create via API; on success **print** Host block + `ssh <name>` hint |
| `wt ls` | List name / status / SSH target |
| `wt rm <name>` | Destroy via API; **print** guidance to remove the Host |
| config / flags | Control-plane URL, token |

## Printed SSH snippet (shape)

```text
Host <name>
  HostName <guest-ip-or-dns>
  User <world-user>
  IdentityFile <key>
```

## Config on the Mac (sketch)

| Item | Where |
|------|--------|
| Control-plane URL, token | `~/.config/wt/config.toml` |

## Language

Rust binary; depends on `wt-api`. HTTP client.

## Failure UX

- Control plane unreachable → clear error; no Host snippet.  
- Create fails → surface error; `ls` can show `Error`; `rm` still cleans server-side state.  

## One-line summary

**Thin CLI: control-plane URL in, Host snippet out, stock `ssh` to enter.**
