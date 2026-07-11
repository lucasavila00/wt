# CLI (`wt`)

Cockpit binary on the Mac (or any client). No Docker.  
Parent: [arch README](./README.md). Control plane: [control-plane.md](./control-plane.md). Server: [bare-metal-agent.md](./bare-metal-agent.md). Crate: [crates/wt-cli](../../crates/wt-cli/README.md) (binary **`wt`**).

## Responsibilities

| Does | Does not |
|------|----------|
| Select a **cluster context** (**SSH target** + optional key) | Run compose, libvirt, or clones on the Mac |
| Call the API via **`ssh … -- helper`** (JSON) as the SSH user | Expose or require a public HTTP control-plane URL |
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

### How the CLI invokes the API (**remote command** — decided)

For **`bare_metal_ssh`**, use **Pattern 1: remote command over SSH**. No port-forward and no public HTTP listener required.

```text
Mac wt
  →  ssh [-i identity_file] [-p port] user@host  --  <server helper>
  →  JSON request on stdin (or argv) / JSON response on stdout
  →  helper runs on the host next to wt-local state / libvirt
```

| Piece | Role |
|-------|------|
| **OpenSSH** | Auth, encryption, remote user (= owner) |
| **`wt` (client)** | Build request, run `ssh …`, parse response, print/sync Hosts |
| **Server helper** | SSH-invoked only; create/list/get/delete; same machine as worker |

Helper name is impl detail (e.g. `wt-local api`). Contract: **JSON in/out over the SSH remote command**, types from `wt-api`.

Port-forward / loopback HTTP are **not** the v1 path.

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

File-based, kube-like **selection**. Each context is a **sum type** (tagged variant): how the CLI reaches a control plane. Only one variant is implemented at first; the config shape leaves room for more.

**Path (sketch):** `~/.config/wt/config.toml`

### Sum type (conceptual / serde)

```text
Context = {
  name: string,
  kind: bare_metal_ssh | …   # discriminant
  // kind-specific fields
}
```

| Kind | Status | Meaning |
|------|--------|---------|
| **`bare_metal_ssh`** | **v1 — only kind** | SSH to a host that runs `wt-local` (or later a plane reachable that way). Auth = SSH; owner = SSH user. |
| **`k8s`** (name TBD) | later | Talk to a k8s-backed worker/plane (kubeconfig context, namespace, …)—**not** defined in detail until that backend exists. |

Unknown `kind` in the file → clear error (“unsupported context kind”), not silent ignore.

### `bare_metal_ssh` (explicit, v1)

```toml
current_context = "home"

[[contexts]]
name = "home"
kind = "bare_metal_ssh"
ssh = "wt@192.168.1.10"              # user@host or SSH config Host alias
# identity_file = "~/.ssh/id_ed25519_wt"  # optional; else agent / defaults
# port = 22                              # optional

[[contexts]]
name = "lab"
kind = "bare_metal_ssh"
ssh = "me@lab.example"
identity_file = "~/.ssh/lab_wt"
```

| Field | Required | Meaning |
|-------|----------|---------|
| `name` | yes | Context id for `use` / `--context` |
| `kind` | yes | Must be `bare_metal_ssh` for this variant |
| `ssh` | yes | OpenSSH target |
| `identity_file` | no | Key for this site |
| `port` | no | SSH port if not 22 |

Later kinds get their **own** required fields (e.g. kube context name)—not overloaded onto `ssh`.

### Example of a future kind (placeholder only)

```toml
# NOT implemented — illustrates sum-type extension only
[[contexts]]
name = "work-k8s"
kind = "k8s"
# kube_context = "dev-eu"
# namespace = "wt"
```

### Selection rules

| Rule | Behavior |
|------|----------|
| Exactly one context | Use it; no flag required |
| Multiple | `current_context`, or `--context <name>`, or `WT_CONTEXT` |
| None / ambiguous | Error with how to fix |

### Context commands

| Command | Behavior |
|---------|----------|
| `wt context list` | Names, **kind**, kind-specific summary (e.g. ssh target), mark current |
| `wt context use <name>` | Set `current_context` |
| `wt context show` | Active context including full variant fields |

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

Wire shape for v1: **stdio (or equivalent) JSON RPC** over the SSH remote command—same `wt-api` payloads as a REST sketch, without public HTTP routes.

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
  config.toml       # contexts: name + kind + kind-specific fields (sum type)
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

**Context is a sum type (`bare_metal_ssh` first); that kind SSHes to the site; owner = SSH user; sync projects my `{repo}-{feature}` guest Hosts.**
