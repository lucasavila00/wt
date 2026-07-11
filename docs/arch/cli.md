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
# remote site (Mac → Ubuntu server)
wt-cli  →  ssh user@host -- wt-local <helper>  →  JSON

# same machine (big Ubuntu workstation / laptop with libvirt)
wt-cli  →  wt-local <helper>                 →  JSON   (no SSH)
```

Same **helper contract** either way. CLI only chooses **how to spawn** the process.

| Concern | Remote (`bare_metal_ssh`) | Local (`bare_metal_local`) |
|---------|---------------------------|----------------------------|
| **Auth / owner** | SSH user | OS user running `wt` / helper |
| **Spawn** | `ssh … -- <helper>` | `<helper>` on `PATH` (or configured path) |
| **API** | JSON stdin/stdout (`wt-api`) | identical |
| **Public HTTP** | no | no |

No separate bearer-token product for bare metal.  
World entry is still **guest** SSH after sync (`Host {repo}-{feature}`)—independent of how you talked to the control plane.

### How the CLI invokes the API (decided)

**Pattern 1: run the server helper as a command**, optionally wrapped in SSH.

```text
# bare_metal_ssh
ssh [-i key] [-p port] user@host --  wt-local <api-args>
     → JSON on stdio

# bare_metal_local  
wt-local <api-args>
     → same JSON on stdio
```

| Piece | Role |
|-------|------|
| **`wt` (client)** | Read context → build argv (`ssh …` or local) → exec → parse JSON → print/sync |
| **Helper (`wt-local`)** | create/list/get/delete; worker on **this** machine |
| **OpenSSH** | Only for remote contexts |

Port-forward / public HTTP are **not** the v1 path.

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
  kind: bare_metal_ssh | bare_metal_local | k8s | …   # discriminant
  // kind-specific fields only
}
```

| Kind | Status | Meaning |
|------|--------|---------|
| **`bare_metal_ssh`** | v1 | Remote host: CLI wraps helper in `ssh user@host -- …`. Owner = SSH user. |
| **`bare_metal_local`** | v1 | **This machine** runs `wt-local` (workstation/laptop/desktop). CLI execs helper directly—**no SSH**. Owner = local OS user. |
| **`k8s`** (name TBD) | later | k8s-backed plane/worker; own fields—not overloaded onto SSH. |

Unknown `kind` → clear error (“unsupported context kind”).

Both bare-metal kinds hit the **same** `wt-local` helper JSON API. Only the **spawn path** differs—so a big Ubuntu box and a Mac→remote Ubuntu share one server implementation.

### `bare_metal_ssh`

```toml
[[contexts]]
name = "remote-lab"
kind = "bare_metal_ssh"
ssh = "wt@192.168.1.10"                 # user@host or SSH Host alias
# identity_file = "~/.ssh/id_ed25519_wt"
# port = 22
```

| Field | Required | Meaning |
|-------|----------|---------|
| `name` | yes | Context id |
| `kind` | yes | `bare_metal_ssh` |
| `ssh` | yes | OpenSSH target |
| `identity_file` | no | Key for this site |
| `port` | no | SSH port if not 22 |

### `bare_metal_local`

```toml
[[contexts]]
name = "this-box"
kind = "bare_metal_local"
# helper = "wt-local"          # optional; default binary name on PATH
# helper_args = ["api"]        # optional; how to invoke the JSON helper
```

| Field | Required | Meaning |
|-------|----------|---------|
| `name` | yes | Context id |
| `kind` | yes | `bare_metal_local` |
| `helper` | no | Path/name of server binary (default e.g. `wt-local`) |

Use when the laptop/desktop **is** the hypervisor (libvirt on the same Ubuntu you develop from). No SSH hop for control-plane ops; guest VMs may still be reached via sync’d Host entries (often local IPs).

### Example of a future kind (placeholder only)

```toml
# NOT implemented — sum-type extension only
[[contexts]]
name = "work-k8s"
kind = "k8s"
# kube_context = "dev-eu"
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
| `wt context list` | Names, **kind**, summary (`ssh` target or `local`), mark current |
| `wt context use <name>` | Set `current_context` |
| `wt context show` | Active context including full variant fields |

Typical multi-machine laptop config: one `bare_metal_local` (this Ubuntu) + one `bare_metal_ssh` (remote lab); `context use` switches.

## Control-plane API (logical)

JSON request/response types in **`wt-api`**. Carried **over SSH** to `wt-local` on the context host.

### Auth

- **`bare_metal_ssh`:** SSH session user → **`owner`**  
- **`bare_metal_local`:** OS user of the helper process → **`owner`**  
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
| `wt new <source> <name>` | Create on current context (ssh or local helper); print **guest** Host snippet when ready |
| `wt ls` | My instances on current context |
| `wt rm <name>` | Destroy; print sync/remove-Host guidance |
| `wt sync` | Project **my** instances with endpoints into managed ssh config (current context) |
| `wt ssh <name>` | Optional: OpenSSH to **guest** Host (world)—not the control-plane hop |
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
  → via context spawn (ssh or local): list my instances
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
| SSH to remote context fails | Clear error (auth, host unreachable)—no API call |
| Local helper missing | Clear error (`wt-local` not on PATH / bad `helper`) |
| Ambiguous context | Error; `context use` |
| Name conflict for me | Conflict error |
| `sync` fails mid-write | Keep previous managed file if replace isn’t complete |

## Language

Rust (`wt-cli` package, binary `wt`). Shells out to **OpenSSH** for remote contexts and for **guest** entry; local contexts exec the helper directly. Depends on `wt-api` for payloads.

## Out of scope (CLI)

- Public HTTPS control plane + OAuth/bearer as the default bare-metal path  
- Admin list-all-users APIs  
- Editing the main body of `~/.ssh/config` (only managed Include target)  

## One-line summary

**Context is a sum type: `bare_metal_ssh` (ssh+helper) or `bare_metal_local` (helper only); same JSON API; sync projects my `{repo}-{feature}` guest Hosts.**
