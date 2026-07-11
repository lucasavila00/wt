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

### 1.5 — Local Git + Compose world

Make the local VM loop run a real repository. Still one Ubuntu workstation. Still no SSH transport.

| Deliver | |
|---------|--|
| `wt-api` | protocol v2; create carries `source`, `name`, optional `git_ref`; instance stores source/ref |
| `wt-cli` | `wt new <source> <name> [--ref <ref>]`; blocking create; print status/IP |
| `wt-local` | SQLite migration; persist source/ref; preserve failure detail |
| `wt-libvirt` | guest exec helper; clone; checkout; Compose discovery; `up --build --wait`; captured errors |
| `wt-setup` | bake `git` + pinned small E2E container image; record package/image provenance |

- Add required `guest.recipe_timeout_seconds` to site config. Development value: `900`.
- One recipe deadline covers clone, checkout, build, and Compose wait.
- Guest commands receive source/ref as argv, never interpolated shell text.
- Keep the final 64 KiB of command stdout/stderr in errors. Prefix phase + exit code.
- Image recipe version changes. `wt-setup image rebuild --config PATH` refuses active `wt-*` domains, then atomically replaces the golden image and manifest. No automatic replacement during install.

#### Git contract

- `source` = Git URL reachable from the guest.
- No `--ref` = remote default branch.
- `--ref` = existing branch, tag, or commit. No branch creation. No push.
- Era 1.5 supports unauthenticated HTTPS and `git://`. No host credential copying. Private Git auth stays out until designed.
- Checkout path = `/workspace/repo` inside the guest.

#### Compose contract

- Look only at repository root.
- Accepted names: `compose.yaml`, `compose.yml`, `docker-compose.yaml`, `docker-compose.yml`.
- Zero matches = error. Multiple matches = error. No guessed precedence.
- Run `docker compose -f <file> up -d --build --wait` from `/workspace/repo`.
- `Running` means clone + checkout + Compose wait succeeded.
- Git/Compose nonzero exit, timeout, or guest loss = `Error`; keep bounded stdout/stderr in `last_error`.
- `.devcontainer` interpretation, overrides, profiles, private credentials, and branch creation are out.

#### Tests

| Lane | Covers |
|------|--------|
| Injected worker | source/ref wire shape, persistence, conflicts, Git/Compose failure propagation |
| KVM | local Git fixture → requested ref → Compose service ready → list → remove |

The KVM fixture is self-contained. Serve a temporary bare Git repository from the host bridge. Its Compose file uses the pinned small image cached by image preparation. No public Git or registry dependency during tests.

**Done when:** local `wt new <source> <name> --ref <ref>` returns `running` only after the selected revision's Compose service is ready; `wt rm` removes containers with the VM.

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
- Guest shell access, `wt ssh`, SSH config sync, multi-node workers, and public APIs stay out.

**Done when:** a client-only machine creates, lists, and removes a Compose-ready world on an Ubuntu site through OpenSSH.

#### Tests

- Config parsing: missing current context, duplicate names, unknown kind, invalid host.
- Transport: exact local/SSH argv, JSON stdin/stdout, remote exit/stderr, protocol mismatch.
- Acceptance: client-only environment invokes a separate Ubuntu site and completes `new` → `ls` → `rm`.

---

### Later (not an era until needed)

- `.devcontainer` interpretation beyond a root Compose file
- Private Git credentials
- Guest SSH, `wt ssh`, and SSH config sync
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
6. Remote context + OpenSSH helper transport

## Open (pick in code)

- Helper argv (`wt-local api` vs flags)  
- Async create/poll after blocking behavior becomes painful
- Private Git credential model

## One-line summary

**Real local VM loop → local Git/Compose world → remote client transport.**
