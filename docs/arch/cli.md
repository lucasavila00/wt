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
| `wt new <source> <name> [--ref <ref>] [--identity PATH]` | Interactively clone an SSH source; start its devcontainer; sync access; print status and Host snippet |
| `wt ls` | List my worlds: name, status, IP, and SSH target |
| `wt rm <name>` | Destroy my world and sync access records |
| `wt sync` | Atomically rewrite the managed SSH config and known-hosts files from my running instance inventory |
| `wt ssh <name>` | Execute stock OpenSSH and enter the primary app container through the synced instance alias |

Era 1 keeps the implemented `wt new <name>` shape. Era 1.5 replaces it with the source/ref form above and adds guest access. The CLI never edits application repositories or mounts their checkout on the host.

## Era 1.5 guest access

- The guest username is the fixed, non-root `wt` user and its working checkout is `/workspace`.
- `wt-local-setup` requires a public-key file path in strict site config. `wt-local` injects those public keys into every world it creates.
- Every world has unique SSH host keys. After boot, `wt-libvirt` reads the public host keys through the QEMU guest agent; `wt-local` persists them with the SSH endpoint.
- `wt sync` manages dedicated config and known-hosts files. It creates `<instance-name>` as an interactive `docker exec -it` shell in the primary app container and `<instance-name>-host` as unrestricted guest SSH. Both pin the same guest host-key identity. The user must place `Include ~/.ssh/wt/config` at the beginning of the main SSH config, before any `Host` blocks. `wt` never edits the user's main SSH config and must not weaken host-key checking.
- VS Code Remote SSH, SCP, and explicit SSH commands use `<instance-name>-host`. Interactive `ssh <instance-name>` and `wt ssh <instance-name>` enter the app container as its configured devcontainer user in the mounted workspace.
- SSH reachability is part of create readiness. `Running` still additionally requires clone, checkout, and Compose wait to succeed.
- Git sources are SSH-only. `--identity` defaults to `~/.ssh/id_ed25519`; `wt-local` prompts for a passphrase when required and never stores it. The key and caller's host trust remain under `/workspace/.git/wt`, allowing Git in both the guest and stock devcontainer to fetch and push. It does not use an ssh-agent.

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
