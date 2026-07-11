# CLI (`wt`)

Era 1/1.5 workstation CLI. Parent: [arch README](./README.md). Helper: [`wt-local`](../../crates/wt-local/).

## Responsibilities

| Does | Does not |
|------|----------|
| Spawn local `wt-local api` | Run libvirt or Docker itself |
| Send one JSON request over stdin | Use SSH to transport control requests |
| Parse one JSON response from stdout | Configure or provision guests over SSH |
| Create / list / remove my worlds | Copy private Git or SSH credentials |
| Project guest SSH inventory into managed OpenSSH files | Export the guest checkout onto the host |

```text
wt  →  local wt-local api  →  wt-libvirt  →  KVM guest
```

Owner = local OS user running the helper.

## Context

Era 1 is local only. No client config. No context selection. `wt-local` resolves from `PATH`.

## Commands

| Command | Behavior |
|---------|----------|
| `wt new <source> <name> [--ref <ref>]` | Clone selected revision; start Compose; print name, status, IP, and SSH Host snippet |
| `wt ls` | List my worlds: name, status, IP, and SSH target |
| `wt rm <name>` | Destroy my world |
| `wt sync` | Atomically rewrite the managed SSH config and known-hosts files from my running instance inventory |
| `wt ssh <name>` | Execute stock OpenSSH using the synced instance alias |

Era 1 keeps the implemented `wt new <name>` shape. Era 1.5 replaces it with the source/ref form above and adds guest access. The CLI never edits application repositories or mounts their checkout on the host.

## Era 1.5 guest access

- The guest username is the fixed, non-root `wt` user and its working checkout is `/workspace`.
- `wt-setup` requires a public-key file path in strict site config. `wt-local` injects those public keys into every world it creates.
- Every world has unique SSH host keys. After boot, `wt-libvirt` reads the public host keys through the QEMU guest agent; `wt-local` persists them with the SSH endpoint.
- `wt sync` manages dedicated config and known-hosts files, ensures the user's main SSH config contains one bounded `Include` for the managed config, and makes `Host <instance-name>` resolve to the recorded guest. It must not weaken host-key checking or overwrite unrelated user SSH configuration.
- VS Code Remote SSH uses the same instance alias and opens `/workspace`, so the editor terminal and Git operations run inside the world.
- SSH reachability is part of create readiness. `Running` still additionally requires clone, checkout, and Compose wait to succeed.
- SSH Git sources require `--identity PATH`. `wt-local` prompts for a passphrase when required, uses the caller's existing SSH host trust, and removes credentials from guest tmpfs immediately after clone. It does not use an ssh-agent.

## Era 2

- Add local and OpenSSH context kinds.
- Select a named context before spawning the helper.
- Keep request/response behavior identical across transports.
- Reuse the guest SSH inventory and access behavior introduced in Era 1.5. Do not add public HTTP.

```toml
current_context = "lab"

[[contexts]]
name = "lab"
kind = "bare_metal_ssh"
host = "wt-lab"
```

Remote invocation: `ssh -- wt-lab wt-local api`.

## One-line summary

**Run and enter the real recipe locally; then carry the same helper API over OpenSSH.**
