# Implementation plan

Order of work. Product: [plan.md](../plan.md). Arch: [arch/](../arch/README.md). CLI: [arch/cli.md](../arch/cli.md).

Crates: `wt-api`, `wt-local`, `wt-libvirt`, `wt-cli` (binary `wt`), `wt-local-setup`, `wt-integration-tests`.

## Division of labor

| Piece | Role |
|-------|------|
| **`wt-api`** | Shared JSON request/response + status enums |
| **`wt-local`** | Site brain: helper + registry + instance service. JSON in → work → JSON out |
| **`wt-libvirt`** | Production libvirt/KVM world backend |
| **`wt-cli` (`wt`)** | Thin: spawn local helper → print |
| **`wt-local-setup`** | Strict Ubuntu/KVM local-site config + install + golden image build |
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
| `wt-local-setup` | config-first Ubuntu install; pinned image; KVM golden build; provenance; drift checks |

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

### 1.5 — Local Git + devcontainer world with interactive access

Make the local VM loop run a real repository and expose the resulting usable development environment through guest SSH. Still one Ubuntu workstation. There is still no SSH transport between `wt` and `wt-local`.

| Deliver | |
|---------|--|
| `wt-api` | protocol v2; create carries SSH `source`, `name`, optional `git_ref`, and identity path; instance stores source/ref plus SSH endpoint and public host keys |
| `wt-cli` | `wt new <source> <name> [--ref <ref>] [--identity PATH]`; blocking interactive create; automatic sync; `wt ssh` |
| `wt-local` | versioned SQLite registry; persist source/ref and SSH identity; preserve failure detail |
| `wt-libvirt` | guest exec helper; SSH setup/readiness; clone; checkout; `devcontainer up`; captured errors |
| `wt-local-setup` | bake `git` + `openssh-server` + pinned Dev Container CLI; configure public-key source; record provenance |

- Add required `guest.recipe_timeout_seconds` to site config. Development value: `900`.
- Add required `guest.ssh_authorized_keys_file` to site config. It points to one or more public keys to inject per world; private keys are rejected.
- One recipe deadline covers clone, checkout, and `devcontainer up`.
- Guest commands receive source/ref as argv, never interpolated shell text.
- Keep the final 64 KiB of command stdout/stderr in errors. Prefix phase + exit code.
- Image recipe version changes. `wt-local-setup image rebuild --config PATH` refuses active `wt-*` domains, then atomically replaces the golden image and manifest. No automatic replacement during install.

#### Git contract

- `source` = `ssh://` or standard scp-style SSH Git URL reachable from the guest. HTTPS, `git://`, and local paths are rejected.
- No `--ref` = remote default branch.
- `--ref` = existing branch, tag, or commit. No branch creation. `wt` never pushes; an interactive user may use Git normally after entering the world.
- `--identity` defaults to `~/.ssh/id_ed25519`. `wt-local` prompts on `/dev/tty` for its passphrase only when required.
- The passphrase exists only in process memory and guest tmpfs during clone. It is never stored in JSON, SQLite, cloud-init, the image, or the world disk.
- After clone, the original private key and caller's host trust are installed under `/workspace/.git/wt`. A repository-local `core.sshCommand` uses that bundle from the guest and devcontainer. Encrypted keys remain encrypted and prompt normally during later fetch/push operations.
- The credential bundle is readable inside the world's trusted devcontainer. Its wrapper makes a mode-`0600` per-command key copy for OpenSSH and deletes that copy afterward. An unencrypted input key therefore grants its full identity to the trusted world/container until `wt rm`.
- Missing identity, invalid passphrase, authentication failure, or unknown host fails with an actionable Git-phase error. There is no ssh-agent or fallback credential mechanism.
- Checkout path = `/workspace` inside the guest.

#### Devcontainer contract

- Run the pinned Dev Container CLI as `devcontainer up --workspace-folder /workspace`.
- The CLI discovers and applies the repository's stock `devcontainer.json`, including its Compose file, workspace mount, Features, users, and lifecycle commands.
- Add no WT-specific config, generated override, or path rewriting. A relative mount such as `..:/workspaces/repo` resolves from the repository's `.devcontainer` directory.
- The checkout's `.git/wt` credential bundle is naturally visible through the stock workspace mount. Provisioning verifies it is readable and executable inside the devcontainer before `Running`.
- `Running` means SSH readiness, clone, checkout, and `devcontainer up` succeeded.
- Git/devcontainer failure, timeout, or guest loss = `Error`; keep bounded stdout/stderr in `last_error`.

#### Guest access contract

- Guest SSH is an Era 1.5 deliverable. SSH transport from `wt` to `wt-local` remains Era 2.
- The fixed guest login is non-root user `wt`; its checkout is `/workspace`.
- `guest.ssh_authorized_keys_file` is strict site configuration. `wt-local-setup` validates the referenced public keys and `wt-libvirt` injects them into each new world's cloud-init data.
- Each world generates unique SSH host keys. After sshd starts, retrieve the public host keys through the QEMU guest agent and persist them with `ssh_user = "wt"`, guest address, and port `22`.
- Treat the host keys, not a DHCP address, as the world's stable SSH identity. Reconciliation refreshes the persisted address from libvirt; `wt sync` then updates the alias without accepting a different host key.
- SSH readiness is required before `Running`. Clone, checkout, and devcontainer provisioning continue through the guest agent.
- `wt new` prints the app-shell and guest-host aliases. `wt ls` shows the SSH endpoint.
- Every `wt new` and `wt rm` runs sync automatically. Explicit `wt sync` atomically derives dedicated managed SSH config and known-hosts files from running instances, enforces host-key checking, and removes entries for removed worlds. The user places `Include ~/.ssh/wt/config` at the beginning of the main SSH config; `wt` never edits that file.
- `wt sync` emits `<name>` with a forced TTY and a remote app-shell command, plus unrestricted `<name>-host`; both use the same pinned guest host keys. The guest app-shell helper resolves the current primary container by its Dev Container CLI workspace label, maps `/workspace` to the container workspace, honors the configured devcontainer user, and runs `docker exec -it` with `/bin/sh`.
- `wt ssh <name>` delegates to stock OpenSSH and enters the app container. VS Code Remote SSH, SCP, and explicit SSH commands use `<name>-host`.
- The repository exists only inside the guest. Era 1.5 adds no virtiofs/9p export, host worktree, or dual host/guest Git state.
- Agent forwarding is not enabled.

#### Tests

| Lane | Covers |
|------|--------|
| Injected worker | source/ref and SSH wire shape, persistence, conflicts, sync inventory, Git/devcontainer/SSH failure propagation |
| KVM | SSH-served jsdev sample → requested ref → devcontainer ready → push from container → strict guest SSH → list → remove |

The KVM test clones `git@github.com:lucasavila00/jsdev-sample.git` into a temporary bare repository and serves it over SSH from the host bridge. The world itself always clones through SSH. It runs the stock devcontainer recipe and proves the container can push a branch back through the installed identity.

The test uses temporary SSH server, Git identity, guest-login identity, and host keys. No long-lived provider credential is required.

**Done when:** local `wt new <source> <name> --ref <ref>` returns `running` only after the selected revision's devcontainer and SSH are ready; `ssh <name>` opens the primary app container, `<name>-host` provides guest and VS Code access, and `wt rm` removes the world and automatically removes both access records.

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
- Git clone and devcontainer execution remain inside the guest on the site.
- Guest shell behavior from Era 1.5 stays unchanged; multi-node workers and public APIs stay out.

**Done when:** a client-only machine creates and lists a devcontainer-ready world on an Ubuntu site through OpenSSH, syncs its guest SSH target, enters it, and removes it.

#### Tests

- Config parsing: missing current context, duplicate names, unknown kind, invalid host.
- Transport: exact local/SSH argv, JSON stdin/stdout, remote exit/stderr, protocol mismatch.
- Acceptance: client-only environment invokes a separate Ubuntu site and completes `new` → `ls` → `sync` → guest SSH → `rm`.

---

### Later (not an era until needed)

- Standalone Compose recipes without `devcontainer.json`
- Shared-site credential lifecycle beyond the trusted Era 1.5 workstation
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
5. Git clone + ref checkout + `devcontainer up`
6. Guest SSH identity + inventory + `wt sync` / `wt ssh`
7. Remote context + OpenSSH helper transport

## Open (pick in code)

- Helper argv (`wt-local api` vs flags)  
- Async create/poll after blocking behavior becomes painful
- Shared-site credential lifecycle; Era 1.5 intentionally copies the selected identity into each trusted world

## One-line summary

**Real local VM loop → usable devcontainer world over guest SSH → remote client transport.**
