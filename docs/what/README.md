# What WT does

WT creates named, parallel development environments from an existing
devcontainer recipe. Each environment is called a world.

## A world

Suppose a repository named `foo` contains `.devcontainer/devcontainer.json`.
Running:

```text
wt new git@github.com:example/foo.git foo-feature
```

creates a VM, clones `foo` into `/workspace`, and starts the environment defined
by that file. If the recipe uses Docker Compose, it starts the referenced
services. The primary devcontainer is the container that VS Code or the Dev
Container CLI would normally open for development.

Each world contains:

- One Ubuntu KVM guest with its own kernel, disk, network, and SSH identity.
- One Git checkout at `/workspace`.
- One Docker daemon with its own containers, images, volumes, and networks.
- The running containers defined by the repository's devcontainer recipe.
- A persistent tmux or Byobu session.
- Separate SSH servers for the guest and primary devcontainer.

The repository remains unchanged. WT adds the app SSH server and access material
while starting the devcontainer.

## Components

| Location | Tools | Role |
|----------|-------|------|
| Workstation | `wt`, OpenSSH, optional VS Code Remote-SSH | Manage and enter worlds |
| Server | OpenSSH, `wt-server`, SQLite, libvirt, QEMU/KVM, registry proxy | Store world state and run VMs |
| Guest | Ubuntu, cloud-init, QEMU guest agent, Git, OpenSSH, Docker Engine, Buildx, Compose, Dev Container CLI, tmux or Byobu | Host one checkout and devcontainer |
| Primary devcontainer | Repository tooling and an injected OpenSSH server | Run the development environment |
| Git host | Existing SSH Git service | Supply the repository |

## Operations

| Command | Result |
|---------|--------|
| `wt new SOURCE NAME` | Create a world and wait for it to become usable |
| `wt logs NAME` | Replay and follow provisioning output |
| `wt ls` | List worlds across configured contexts |
| `wt rm NAME` | Destroy a world |
| `wt sync` | Update managed OpenSSH aliases (`new`, `ls`, and `rm` do this automatically; run `sync` on another workstation after changing worlds elsewhere) |

A context identifies a WT server. `context.world` is the stable world name.
Short names work when globally unique. Git sources use `ssh://...` or
`user@host:path`.

## Connections

| Action | Route | Result |
|--------|-------|--------|
| Local `wt` command | `wt` → local `wt-server` | Manage worlds on the same Ubuntu/KVM machine |
| Remote `wt` command | Workstation → server SSH → `wt-server` | Manage worlds without a WT network service |
| `ssh NAME-host` | Workstation → guest SSH | Guest shell and recovery |
| `ssh NAME` | Workstation → guest SSH → tmux/Byobu → app SSH | Persistent session inside the primary devcontainer |
| `ssh NAME-dc` | Workstation → guest proxy → app SSH | Direct devcontainer access for VS Code, commands, and file transfer |
| Git operation | Guest or app → Git host SSH | Clone, fetch, and push repository data |

## Safety model

| Property | Enforcement |
|----------|-------------|
| Control-plane exposure | WT opens no control-plane port. Local contexts execute `wt-server`; remote contexts execute it through OpenSSH. |
| World isolation | Every world is a separate KVM guest with its own kernel, disk overlay, network identity, and Docker daemon. Worlds share no host Docker daemon, container namespace, writable layer, or checkout. |
| SSH authentication | Server access follows the user's OpenSSH policy. Guest and app access require configured public keys. |
| SSH identity | Every world gets unique guest and app host keys. WT verifies and pins both identities with strict host-key checking. |
| App SSH exposure | The app SSH server is reached through the guest proxy; no app SSH port is published on the KVM host. |
| Git credentials | The server key must be encrypted. The passphrase is read on the workstation, used for provisioning, and never persisted. Git host keys are pinned. |
| Configuration | Setup installs one strict server config and fails when installed state drifts from it. |

### Trust boundaries

- The KVM host, its administrators, and users with libvirt control are trusted.
  They can inspect or control worlds.
- The deployment is for one developer or a trusted team, not hostile tenants.
- Repository and devcontainer code is trusted inside its world. It can access
  that world's checkout and checkout-local encrypted Git identity.
- Anyone holding an authorized world SSH key can access the guest and app.
- The application recipe may publish its own ports inside the world. Host
  routing and firewall policy determine whether those ports are reachable.

## Requirements and limits

- Ubuntu 24.04 amd64 servers with KVM.
- App images derived from Debian or Ubuntu with `apt`.
- The stock devcontainer recipe remains the environment contract.
- No KVM emulation fallback.

WT is not a CI system, Git worktree manager, recipe language, or hosted IDE.

Implementation: [How WT works](../how/README.md).
