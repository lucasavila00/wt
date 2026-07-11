# CLI (`wt`)

Cockpit binary on the Mac (or any client). No Docker.  
Parent: [arch README](./README.md). Control plane: [control-plane.md](./control-plane.md). Server: [bare-metal-agent.md](./bare-metal-agent.md). Crate: [crates/wt-cli](../../crates/wt-cli/README.md) (binary **`wt`**).

## Responsibilities

| Does | Does not |
|------|----------|
| Select a **cluster context** (**SSH target** + optional key) | Run compose, libvirt, or clones on the Mac |
| Call the **control-plane API over SSH** as the SSH user | Expose or require a public HTTP control-plane URL |
| Create / list / destroy **my** instances | See other users’ instances (single-tenant client) |
| **Print** SSH Host snippets; **`wt sync`** into managed ssh config | Treat the laptop as inventory source of truth |
| Optional **`wt ssh <instance>`** into a **world** | Replace OpenSSH |

## Transport and auth: SSH first

Client → control plane uses **SSH as much as possible**.

```text
Mac wt-cli
   │  ssh -i optional_key  user@hypervisor
   ▼
hypervisor: wt-local (API on localhost / unix socket / ssh forced-command)
   │
   ▼
libvirt worlds …   (separate: ssh Host after sync → guest)
```

| Concern | Approach |
|---------|----------|
| **Auth** | SSH public-key (or agent); optional per-context `IdentityFile` |
| **Identity / owner** | SSH user (or cert principal) on the site host → instance `owner` |
| **API surface** | Same logical JSON instance CRUD; **not** a public internet listener |
| **Reachability** | If you can SSH to the site host, you can use `wt` |

No separate bearer-token product for bare-metal / `wt-local`.  
World access after provision is still normal **guest** SSH (`Host {repo}-{feature}`)—that is a **second** hop, same OpenSSH tooling.

### How the CLI invokes the API (implementation choices)

Any of these is fine; pick one in impl and keep it boring:

| Mechanism | Idea |
|-----------|------|
| **Remote subcommand** | `ssh … wt-local-ctl …` or `ssh … -- wt-api` with JSON on stdio |
| **Local forward** | `ssh -L` to loopback HTTP on the host, CLI talks to `127.0.0.1` |
| **Unix socket + ssh** | Stream over SSH to a socket only root/service user can open |

Docs care that **auth and path are SSH**, not which of the three.

## Tenancy model

| Side | Model |
|------|--------|
| **Server host** | Multi-user OS accounts (or SSH principals); many owners’ instances |
| **Client** | One SSH identity per context |

Every instance has **`owner`** = authenticated SSH identity. List / sync / destroy default to **`owner == me`**.

## Identity: cluster × repo × feature

| Concept | Meaning |
|---------|---------|
| **Cluster** | A site you can SSH into (context). Where `wt-local` runs. |
| **Repo** | Short name from source (e.g. `frontend`) |
| **Feature** | Slug (often branch), e.g. `checkout-rewrite` |
| **Instance name** | **`{repo}-{feature}`** e.g. `frontend-checkout-rewrite` |

Instance name = control-plane resource name (unique **per owner** on that cluster) = SSH **`Host`** after sync.

**v1 `wt sync`:** current context only → Host = instance name is enough.

## Contexts (which cluster)

File-based, kube-like **selection**, but fields are **SSH**, not URL+token.

**Path (sketch):** `~/.config/wt/config.toml`

```toml
current_context = "home"

[[contexts]]
name = "home"
ssh = "wt@192.168.1.10"          # required: user@host (or SSH Host alias)
# identity_file = "~/.ssh/id_ed25519_wt"   # optional; else ssh agent / default keys
# port = 22                                # optional

[[contexts]]
name = "lab"
ssh = "me@lab.example"
identity_file = "~/.ssh/lab_wt"
```

| Rule | Behavior |
|------|----------|
| Exactly one context | Use it; no flag required |
| Multiple | `current_context`, or `--context <name>`, or `WT_CONTEXT` |
| None / ambiguous | Error with how to fix |
| `ssh` value | OpenSSH target: `user@host`, or a `Host` alias from the user’s ssh config |

### Context commands

| Command | Behavior |
|---------|----------|
| `wt context list` | Names, `ssh` targets, mark current |
| `wt context use <name>` | Set `current_context` |
| `wt context show` | Active context (ssh target, key path if set) |

## Control-plane API (logical)

JSON request/response types in **`wt-api`**. Carried **over SSH** to `wt-local` on the context host.

### Auth

- Established by the **SSH session**  
- Server trusts the connected user → **`owner`**  
- No Bearer token in the v1 client protocol  

### Instance fields (conceptual)

| Field | Meaning |
|-------|---------|
| `name` | `{repo}-{feature}` |
| `owner` | SSH user / principal |
| `source` | git source string from `new` |
| `ref` | optional git ref |
| `status` | `Provisioning` \| `StartingRecipe` \| `Running` \| `Error` \| `Destroying` (etc.) |
| `endpoint` | **Guest** SSH: `user`, `host`, `port` (for world entry after sync) |
| `last_error` | optional |

### Operations

| Op | Behavior |
|----|----------|
| **Create** | `{ source, name, ref? }`; `owner = me`; 409 if I already have `name` |
| **List** | My instances only |
| **Get** | Mine by name |
| **Delete** | Mine by name |

Wire paths (if HTTP-on-loopback behind SSH) may look like `/v1/instances`; if stdio RPC, same payloads without public routes. **Semantics** matter more than URL cosmetics.

## Commands

| Command | Behavior |
|---------|----------|
| `wt new <source> <name>` | Create on current cluster (over SSH); print **guest** Host snippet when endpoint ready |
| `wt ls` | My instances on current cluster |
| `wt rm <name>` | Destroy; print sync/remove-Host guidance |
| `wt sync` | Project **my** instances with endpoints into managed ssh config (current context) |
| `wt ssh <name>` | Optional: OpenSSH to **guest** Host (world), not the control-plane host |
| `wt context list\|use\|show` | Cluster selection |
| `--context <name>` | Per-invocation cluster override |

## SSH integration (worlds)

Control-plane SSH ≠ world SSH.

| Hop | Purpose |
|-----|---------|
| Context `ssh = user@hypervisor` | Operate `wt` API / `wt-local` |
| Host `frontend-…` → guest IP | Live inside the dev world |

### Managed file + Include

| Piece | Path (sketch) |
|-------|----------------|
| Managed world Hosts | `~/.config/wt/ssh_config` (tool-owned) |
| User once | `Include config` from `~/.ssh/config` |

### Print

After successful `new`, print pasteable **guest** Host block (`Host {name}`, `HostName` = guest IP, etc.).

### `wt sync`

```text
wt sync
  → over context SSH: list my instances
  → rewrite managed file: Host {name} → guest endpoint
  → drop Hosts for instances I no longer have
```

Only entries with a usable guest `endpoint`. Current context only (v1).

### `wt ssh <name>`

Convenience into the **world** (same as `ssh <name>` after sync).

## Config layout

```text
~/.config/wt/
  config.toml       # contexts: name, ssh, optional identity_file
  ssh_config        # managed world Hosts (from sync)
  keys/             # optional; often reuse existing SSH keys
```

## Failure UX

| Case | Behavior |
|------|----------|
| SSH to cluster fails | Clear error (auth, host unreachable)—no API call |
| Ambiguous context | Error; `context use` |
| Name conflict for me | Conflict error |
| `sync` fails mid-write | Keep previous managed file if replace isn’t complete |

## Language

Rust (`wt-cli` package, binary `wt`). Shells out to **OpenSSH** for transport and world entry. Depends on `wt-api` for payloads.

## Out of scope (CLI)

- Public HTTPS control plane + OAuth/bearer as the default bare-metal path  
- Admin list-all-users APIs  
- Editing the main body of `~/.ssh/config` (only managed Include target)  

## One-line summary

**Context = SSH target (+ optional key); API runs over that SSH hop; owner = SSH user; sync projects my `{repo}-{feature}` guest Hosts for normal `ssh`.**
