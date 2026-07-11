# Implementation plan

Order of work. Product: [plan.md](../plan.md). Arch: [arch/](../arch/README.md). CLI: [arch/cli.md](../arch/cli.md).

Crates: `wt-api`, `wt-local`, `wt-libvirt`, `wt-cli` (binary `wt`), `wt-setup`, `wt-integration-tests`.

## Division of labor

| Piece | Role |
|-------|------|
| **`wt-api`** | Shared JSON request/response + status enums |
| **`wt-local`** | Site brain: helper + registry + instance service. JSON in → work → JSON out |
| **`wt-libvirt`** | Production libvirt/KVM world backend |
| **`wt-cli` (`wt`)** | Thin: spawn local helper → print |
| **`wt-setup`** | Strict Ubuntu/KVM site config + install + golden image build |
| **`wt-integration-tests`** | Injected service tests + real libvirt/KVM acceptance test |

**Real VMs (libvirt/KVM) are the hard part.**

```text
wt-cli  →  wt-local helper  →  JSON
```

## Eras

### 1 — Local loop + libvirt VMs

Ship the shape around the hard part: types + helper + CLI + real guests on **one machine**.

| Deliver | |
|---------|--|
| `wt-api` | create / list / get / delete; status; guest IP; errors |
| `wt-local` | helper entrypoint; durable local registry; owner = process user; instance service; `Provisioning` → `Running` / `Error` |
| `wt-libvirt` | Docker + Compose golden image; define/start/destroy; guest-agent readiness; guest IP; instance↔domain; KVM only |
| `wt-cli` | `new` / `ls` / `rm`; spawn helper; print status/IP |
| `wt-setup` | config-first Ubuntu install; pinned image; KVM golden build; provenance; drift checks |

**Done when:** local `wt new` creates a real KVM guest where the guest agent verifies Docker Engine + Compose; `wt ls` shows it; `wt rm` destroys it.

**Out:** clone/recipe execution, all SSH, remote contexts, multi-node, public HTTP.

#### Tests

| Lane | Covers |
|------|--------|
| **Injected worker** | helper/API, registry, ownership, state transitions, restart persistence, failures |
| **Libvirt/KVM** | production backend: image, disk, domain, boot, guest agent, Docker + Compose, IP, destroy |

Both lanes live in `wt-integration-tests`. The injected worker is fast and deterministic. It does not imitate libvirt internals.

The libvirt/KVM test is the Era 1 acceptance test: `wt new` → `wt ls` → `wt rm`.

Keep unit tests narrow: validation and wire types.

---

### 1.5 — Local Git + Compose world with interactive access

Make the local VM loop run a real repository and expose the resulting usable development environment through guest SSH. Still one Ubuntu workstation. There is still no SSH transport between `wt` and `wt-local`.

| Deliver | |
|---------|--|
| `wt-api` | protocol v2; create carries `source`, `name`, optional `git_ref`; instance stores source/ref plus SSH endpoint and public host keys |
| `wt-cli` | `wt new <source> <name> [--ref <ref>]`; blocking create; print status/IP/Host snippet; `wt sync`; `wt ssh` |
| `wt-local` | SQLite migration; persist source/ref and SSH identity; preserve failure detail |
| `wt-libvirt` | guest exec helper; SSH setup/readiness; clone; checkout; Compose discovery; `up --build --wait`; captured errors |
| `wt-setup` | bake `git` + `openssh-server` + pinned small E2E container image; configure public-key source; record package/image provenance |

- Add required `guest.recipe_timeout_seconds` to site config. Development value: `900`.
- Add required `guest.ssh_authorized_keys` to site config. It contains one or more public keys to inject per world; private keys are rejected.
- One recipe deadline covers clone, checkout, build, and Compose wait.
- Guest commands receive source/ref as argv, never interpolated shell text.
- Keep the final 64 KiB of command stdout/stderr in errors. Prefix phase + exit code.
- Image recipe version changes. `wt-setup image rebuild --config PATH` refuses active `wt-*` domains, then atomically replaces the golden image and manifest. No automatic replacement during install.

#### Git contract

- `source` = Git URL reachable from the guest.
- No `--ref` = remote default branch.
- `--ref` = existing branch, tag, or commit. No branch creation. `wt` never pushes; an interactive user may use Git normally after entering the world.
- The gating Era 1.5 path supports unauthenticated HTTPS and `git://`. It is validated with a self-contained local fixture and does not depend on external credentials.
- For early private-repository usage, accept `ssh://` and standard scp-style Git sources and provisionally rely on the invoking site user's loaded `ssh-agent`. Forward agent access into the guest only for the clone; never copy private keys into the API, registry, image, cloud-init data, or world disk.
- Assume `wt-local` inherits a usable `SSH_AUTH_SOCK`, a matching key is loaded, and the invoking owner's SSH host trust covers the real hostname in `source`. Missing agent, authentication failure, or unknown host fails with an actionable Git-phase error; there is no fallback credential mechanism.
- This agent behavior is a convenience bridge, not the settled credential architecture. Do not let deploy-key lifecycle, provider integration, shared-site policy, or credential hardening block the core Era 1.5 implementation and acceptance test.
- Checkout path = `/workspace/repo` inside the guest.

#### Compose contract

- Look only at repository root.
- Accepted names: `compose.yaml`, `compose.yml`, `docker-compose.yaml`, `docker-compose.yml`.
- Zero matches = error. Multiple matches = error. No guessed precedence.
- Run `docker compose -f <file> up -d --build --wait` from `/workspace/repo`.
- `Running` means SSH readiness plus clone + checkout + Compose wait succeeded.
- Git/Compose nonzero exit, timeout, or guest loss = `Error`; keep bounded stdout/stderr in `last_error`.
- `.devcontainer` interpretation, overrides, profiles, private credentials, and branch creation are out.

#### Guest access contract

- Guest SSH is an Era 1.5 deliverable. SSH transport from `wt` to `wt-local` remains Era 2.
- The fixed guest login is non-root user `wt`; its checkout is `/workspace/repo`.
- `guest.ssh_authorized_keys` is strict site configuration. `wt-setup` validates public-key syntax and `wt-libvirt` injects the keys into each new world's cloud-init data. No private key is copied or generated.
- Each world generates unique SSH host keys. After sshd starts, retrieve the public host keys through the QEMU guest agent and persist them with `ssh_user = "wt"`, guest address, and port `22`.
- Treat the host keys, not a DHCP address, as the world's stable SSH identity. Reconciliation refreshes the persisted address from libvirt; `wt sync` then updates the alias without accepting a different host key.
- SSH readiness is required before `Running`. Core clone/checkout/Compose provisioning continues through the guest agent; the provisional private-SSH clone path may use narrowly scoped agent forwarding after sshd is ready.
- `wt new` prints a usable `Host <name>` snippet. `wt ls` shows the SSH target.
- `wt sync` atomically derives dedicated managed SSH config and known-hosts files from the caller's running instances. It ensures the user's main SSH config contains one bounded `Include` for the managed file, preserves all unrelated configuration, enforces host-key checking, and removes entries for removed worlds.
- `wt ssh <name>` delegates to stock OpenSSH using the managed alias. VS Code Remote SSH uses that same alias and opens `/workspace/repo`. On the trusted Era 1.5 workstation, the managed alias may forward the caller's agent for interactive Git use without placing key material in the world.
- The repository exists only inside the guest. Era 1.5 adds no virtiofs/9p export, host worktree, or dual host/guest Git state.
- The exact private-Git and agent-forwarding model remains explicitly open for later design; Era 1.5 records the assumptions above rather than presenting them as a durable multi-user security contract.

#### Tests

| Lane | Covers |
|------|--------|
| Injected worker | source/ref and SSH wire shape, persistence, conflicts, sync inventory, Git/Compose/SSH failure propagation |
| KVM | local Git fixture → requested ref → Compose service ready → strict host-key SSH login → command in `/workspace/repo` → list → remove |

The KVM fixture is self-contained. Serve a temporary bare Git repository from the host bridge. Its Compose file uses the pinned small image cached by image preparation. Use a test-only SSH keypair, inject only its public key, and verify the reported world host key. No public Git or registry dependency during tests.

The self-contained fixture is the gating acceptance path. When provisional SSH cloning is implemented, add a focused test with a temporary Git SSH server, temporary agent, and known host; do not make a real Git provider or long-lived credential a test dependency.

**Done when:** local `wt new <source> <name> --ref <ref>` returns `running` only after the selected revision's Compose service and SSH are ready; after `wt sync`, `ssh <name>` and VS Code Remote SSH open the usable environment at `/workspace/repo`; `wt rm` removes the world and the next sync removes its access records.

---

### 2 — Remote client + site server

Run `wt` and `wt-local` on different machines. Do not change world or recipe semantics.

| Deliver | |
|---------|--|
| Client config | named `bare_metal_local` and `bare_metal_ssh` contexts |
| Transport | `ssh -- <site> wt-local api`; same versioned stdio JSON |
| CLI | context list/use/show; `new` / `ls` / `rm` over selected context |
| Install | client-only `wt`; server `wt-local` + `wt-libvirt` + existing site setup |
| Auth | existing OpenSSH config/agent; never generate or copy keys |

Client config: `~/.config/wt/config.toml`.

```toml
current_context = "local"

[[contexts]]
name = "local"
kind = "bare_metal_local"

[[contexts]]
name = "lab"
kind = "bare_metal_ssh"
host = "wt-lab"
```

- `host` = OpenSSH destination or config alias. Reject empty values and values starting with `-`.
- Local context always runs `wt-local api` from `PATH`.
- SSH context always runs `ssh -- <host> wt-local api`. No configurable helper argv.
- `--context <name>` overrides `current_context` for one command. `wt context use <name>` rewrites only `current_context`.

- No HTTP listener.
- Remote owner = OS user executing `wt-local` on the site.
- Git clone and Compose remain inside the guest on the site.
- Guest shell behavior from Era 1.5 stays unchanged; multi-node workers and public APIs stay out.

**Done when:** a client-only machine creates and lists a Compose-ready world on an Ubuntu site through OpenSSH, syncs its guest SSH target, enters it, and removes it.

#### Tests

- Config parsing: missing current context, duplicate names, unknown kind, invalid host.
- Transport: exact local/SSH argv, JSON stdin/stdout, remote exit/stderr, protocol mismatch.
- Acceptance: client-only environment invokes a separate Ubuntu site and completes `new` → `ls` → `sync` → guest SSH → `rm`.

---

### Later (not an era until needed)

- `.devcontainer` interpretation beyond a root Compose file
- Final private Git credential, host-trust, and agent-forwarding model
- Modules/bins for multi-node (`wt-control-plane`, `wt-worker`)  
- k8s context kind  
- Runbook polish, destroy policies, fleets  

Do not pre-build these.

---

## First commits

1. `wt-api` types
2. `wt-local` helper + durable local registry
3. `wt-libvirt` KVM guest lifecycle + Docker/Compose readiness
4. `wt-cli` local spawn + new/ls/rm
5. Git clone + ref checkout + Compose up
6. Guest SSH identity + inventory + `wt sync` / `wt ssh`
7. Remote context + OpenSSH helper transport

## Open (pick in code)

- Helper argv (`wt-local api` vs flags)  
- Async create/poll after blocking behavior becomes painful
- Final private Git credential and agent-forwarding model; Era 1.5 uses the provisional assumptions above

## One-line summary

**Real local VM loop → usable local Git/Compose world over guest SSH → remote client transport.**
