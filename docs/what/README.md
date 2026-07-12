# What WT does

WT creates named, parallel development environments from an existing
devcontainer recipe. Each environment is a world with its own checkout, Docker
state, network, and running devcontainer.

## System

```text
workstation
  ├─ wt ── local process or OpenSSH ─► wt-server on an Ubuntu/KVM server
  └─ OpenSSH
       ├─ NAME-host ───► guest SSH server
       ├─ NAME ────────► guest SSH server ─► tmux/Byobu ─► app SSH server
       └─ NAME-dc ─────► guest TCP proxy ────────────────► app SSH server

server
  ├─ wt-server + SQLite
  ├─ libvirt + QEMU/KVM
  ├─ registry cache
  └─ world
       ├─ Ubuntu guest + OpenSSH server
       ├─ /workspace Git checkout
       ├─ Docker + Compose + Dev Container CLI
       └─ primary devcontainer + OpenSSH server
```

A local context runs `wt-server` on the same Ubuntu/KVM machine as `wt`. A
remote context reaches it through the server's existing OpenSSH service. Both
expose the same WT operations.

## Concepts

| Concept | Meaning |
|---------|---------|
| Context | A configured WT server |
| World | One isolated development environment |
| Source | An SSH Git repository cloned into the world |
| Recipe | The repository's existing `devcontainer.json` |

## Tools

| Location | Tools |
|----------|-------|
| Workstation | `wt`, OpenSSH, optional VS Code Remote-SSH |
| Server | `wt-server`, SQLite, libvirt, QEMU/KVM, registry proxy |
| Guest | Ubuntu, cloud-init, QEMU guest agent, Git, OpenSSH, Docker Engine, Buildx, Compose, Dev Container CLI, tmux or Byobu |
| Devcontainer | Repository tooling and an injected OpenSSH server |

## SSH connections

WT uses separate SSH connections for separate jobs:

| Connection | Purpose |
|------------|---------|
| Workstation → server | Run `wt-server api` for a remote context |
| Workstation → guest | Guest shell, recovery, persistent session, and app proxy |
| Workstation → app | Direct devcontainer access through the guest proxy |
| World → Git host | Clone and use the SSH Git source from the guest or app |

The guest and app each have their own SSH server, user, keys, and pinned host
identity. The app SSH server is inside the primary devcontainer and is reached
through the guest; WT does not publish an app SSH port on the KVM host.

The base `NAME` alias attaches to tmux or Byobu in the guest. Each session pane
uses SSH to enter the current primary devcontainer. The `NAME-dc` alias connects
the workstation's OpenSSH client directly to the app SSH server through the
guest proxy.

## Safety model

WT keeps the control plane small and uses existing isolation and authentication
boundaries.

| Property | Enforcement |
|----------|-------------|
| Control-plane exposure | WT opens no control-plane port. Local contexts execute `wt-server`; remote contexts execute it through OpenSSH. |
| World isolation | Every world is a separate KVM guest with its own kernel, disk overlay, network identity, and Docker daemon. |
| SSH authentication | Server access follows the user's OpenSSH policy. Guest and app access require configured public keys. |
| SSH identity | Every world gets unique guest and app host keys. WT verifies and pins both identities with strict host-key checking. |
| App SSH exposure | The app SSH server is reached through the guest proxy; no app SSH port is published on the KVM host. |
| Git credentials | The server key must be encrypted. The passphrase is read on the workstation, used for provisioning, and never persisted. Git host keys are pinned. |
| Configuration | Setup installs one strict server config and fails when installed state drifts from it. |

The KVM guest is the boundary around repository and devcontainer code. Worlds do
not share a host Docker daemon, container namespace, writable layer, or checkout.

### Trust boundaries

- The KVM host, its administrators, and users with libvirt control are trusted.
  They can inspect or control worlds.
- Repository and devcontainer code is trusted inside its world. It can access
  that world's checkout and checkout-local encrypted Git identity.
- Anyone holding an authorized world SSH key can access the guest and app.
- The application recipe may publish its own ports inside the world. Host
  routing and firewall policy determine whether those ports are reachable.
- WT relies on KVM, libvirt, QEMU, OpenSSH, and the host kernel for their stated
  security boundaries.

WT is designed for one developer or a trusted team sharing a server. It is not
a hostile multi-tenant sandbox. SSH protects transport, authentication, and
endpoint identity; it does not validate or sandbox trusted repository code
beyond the world VM boundary.

## Commands

| Command | Result |
|---------|--------|
| `wt new SOURCE NAME` | Create a world and wait for it to become usable |
| `wt logs NAME` | Replay and follow provisioning output |
| `wt ls` | List worlds across configured contexts |
| `wt rm NAME` | Destroy a world |
| `wt sync` | Update managed OpenSSH aliases |

`context.world` is the stable name. Short names work when globally unique.

## Access

| Alias | Target |
|-------|--------|
| `NAME` | Persistent app session |
| `NAME-dc` | Devcontainer for VS Code, commands, and file transfer |
| `NAME-host` | Guest shell and recovery |

## Requirements and limits

- Ubuntu 24.04 amd64 servers with KVM.
- SSH Git sources: `ssh://...` or `user@host:path`.
- App images derived from Debian or Ubuntu with `apt`.
- Trusted server users; no hostile multi-tenant isolation.
- The stock devcontainer recipe remains the environment contract.
- No KVM emulation fallback.

WT is not a CI system, Git worktree manager, recipe language, or hosted IDE.

Implementation: [How WT works](../how/README.md).
