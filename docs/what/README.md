# What WT does

WT creates named, parallel development environments from an existing
devcontainer recipe. Each environment is called a world.

## A world

Suppose a repository named `foo` contains `.devcontainer/devcontainer.json`.
Running `wt new` opens an interactive prompt for the world name, Git source,
revision, CPU, RAM, disk, and confirmation:

```text
wt new
```

creates a VM, clones `foo` into `/workspace`, and starts the environment defined
by that file. If the recipe uses Docker Compose, it starts the referenced
services. The primary devcontainer is the container that VS Code or the Dev
Container CLI would normally open for development.

By default, WT checks out the repository's default branch. The revision prompt
also accepts:

- `branch:BRANCH` checks out a branch with an attached HEAD. New commits made
  in the world are added to that branch.
- `ref:REF` checks out a tag, commit SHA, or other Git commit-ish with a
  detached HEAD. Use this to create a world pinned to a specific revision.

WT performs the checkout before it reads and starts the devcontainer recipe, so
the selected revision supplies `.devcontainer/devcontainer.json` and any files
referenced by that recipe.

The resource prompts default to 2 CPUs, 4096 MiB RAM, and 32 GiB disk. WT reads
and authorizes every valid regular `~/.ssh/*.pub` file from the workstation.

Each world contains:

- One Ubuntu KVM guest with its own kernel, disk, network, and SSH identity.
- One Git checkout at `/workspace`.
- One Docker daemon with its own containers, images, volumes, and networks.
- The running containers defined by the repository's devcontainer recipe.
- A persistent Byobu session.
- Separate SSH servers for the guest and primary devcontainer.

The repository remains unchanged. WT adds the app SSH server and access material
while starting the devcontainer.

## Components

| Location | Tools | Role |
|----------|-------|------|
| Workstation | `wt`, OpenSSH, optional VS Code Remote-SSH | Manage and enter worlds |
| Server | OpenSSH, `wt-server`, SQLite, libvirt, QEMU/KVM, registry proxy | Store world state and run VMs |
| Guest | Ubuntu, cloud-init, QEMU guest agent, Git, OpenSSH, Docker Engine, Buildx, Compose, Dev Container CLI, Byobu | Host one checkout and devcontainer |
| Primary devcontainer | Repository tooling and an injected OpenSSH server | Run the development environment |
| Git host | Existing SSH Git service | Supply the repository |

## Operations

| Command | Result |
|---------|--------|
| `wt new` | Interactively create a guest and wait for its setup SSH endpoint |
| `wt ls` | List worlds across configured contexts |
| `wt code NAME` | Open the running world's mounted workspace in VS Code Remote-SSH |
| `wt rm NAME` | Destroy a world |
| `wt sync` | Update managed OpenSSH aliases (`new`, `ls`, and `rm` do this automatically; run `sync` on another workstation after changing worlds elsewhere) |

A context identifies a WT server. `context.world` is the stable world name.
Short names work when globally unique. Git sources use `ssh://...` or
`user@host:path`.

`wt code ars.jsdev` refreshes the managed SSH inventory, discovers the primary
devcontainer's current workspace mount, and runs the local VS Code CLI against
the `ars.jsdev-vs` Remote-SSH alias. It requires the `code` command and VS Code's
Remote-SSH extension on the workstation.

## SSH access

### `ssh NAME`

1. The workstation's SSH connection terminates at the guest SSH server and
   verifies its host key. For a remote context, OpenSSH reaches the guest's
   private address through the context server as a jump host.
2. The guest runs `wt-app-shell`, which attaches to Byobu in the guest. On the first connection it starts the world installer using the workstation's forwarded SSH agent.
3. Each pane runs `wt-app-pane`. It finds the current primary devcontainer and
   opens a separate guest-to-app SSH connection with a guest-held session key.
4. The pane opens a login shell at the mounted workspace.

`NAME` and `NAME-vs` require the injected app SSH server. `NAME`
does not need a TCP proxy: `wt-app-pane` runs inside the guest, where it can
connect directly to the app's private Docker address.

Byobu stays in the guest when the workstation disconnects. It does not need to
be provided by the devcontainer. If the
devcontainer stops, the pane's SSH connection ends; new panes resolve the
current container when it is running again.

When Byobu is selected, WT sets the terminal title to the qualified world and
repository name, such as `ars.wt2 — repo`. The qualified name remains the same
when the world is reached through its unqualified SSH alias.

### `ssh NAME-vs`

1. The workstation's main OpenSSH client starts a proxy command.
2. The proxy command opens its own SSH connection to the guest and runs
   `wt-app-proxy`.
3. The proxy forwards bytes to the app SSH server inside the primary
   devcontainer.
4. The main OpenSSH connection terminates at the app SSH server. It verifies the
   app host key and authenticates with the workstation's configured world key.

This connection has no forced session command. Use it for VS Code Remote-SSH,
commands, SFTP, and forwarding.

The proxy is required because the app has a private Docker address inside the
guest. That address is not directly reachable from the workstation and may
change when Docker recreates the container. WT does not publish an app SSH port
or modify the repository's port configuration.

On each connection, `wt-app-proxy` finds the current devcontainer, connects to
its private address on port 2222, and relays bytes. It does not terminate the app
SSH connection: the workstation still authenticates directly to the app SSH
server and verifies the app host key.

| Alias | Main SSH endpoint | App login key | Behavior |
|-------|-------------------|---------------|----------|
| `NAME` | Guest SSH server | Guest-held session key | Guest-hosted persistent session; panes enter the app |
| `NAME-vs` | App SSH server through the guest proxy | Workstation's world key | Direct app SSH |
| `NAME-host` | Guest SSH server | Not applicable | Direct guest SSH |

Remote `wt` commands use the server's existing SSH service to run `wt-server`.
World SSH connections use the same configured server destination as an
OpenSSH jump host. The server must allow TCP forwarding and be able to reach
the guests; the workstation does not need a route to the libvirt network.

## Git credentials

The server config points to a Git known-hosts file. Git authentication comes
from the workstation's forwarded SSH agent during setup and later work inside
the devcontainer.

When creating a world, `wt` reads the workstation's global Git `user.name` and
`user.email`. Both values are required. If either is missing, empty, or cannot be
read, world creation stops before contacting the server. Both values are sent in
the create request and written to the checkout's local Git config. WT does not
copy other Git configuration.

After `wt new` returns, the first `ssh NAME` forwards the workstation agent and
starts the installer in Byobu. The installer clones with strict host-key
checking, finishes package and Docker setup, starts the devcontainer, and tees
its output to both Byobu and a guest-held log. Clone trust is removed after
checkout; the narrowly scoped setup privilege and remaining inputs are removed
before completion. Later connections forward the agent into the devcontainer.
No private key or passphrase crosses the WT API or remains in the world.

## Safety model

| Property | Enforcement |
|----------|-------------|
| Control-plane exposure | WT opens no control-plane port. A mode-`0600` Unix socket serves local bridges; remote contexts reach a bridge through OpenSSH. |
| World isolation | Every world is a separate KVM guest with its own kernel, disk overlay, network identity, and Docker daemon. Worlds share no host Docker daemon, container namespace, writable layer, or checkout. |
| SSH authentication | Server access follows the user's OpenSSH policy. Guest and app access require configured public keys. |
| SSH identity | Every world gets unique guest and app host keys. WT verifies and pins both identities with strict host-key checking. |
| App SSH exposure | The app SSH server is reached through the guest proxy; no app SSH port is published on the KVM host. |
| Git credentials | SSH connections forward the workstation agent into the devcontainer. The socket dies with the connection. Temporary clone known hosts are removed after checkout. |
| Configuration | Setup installs one strict server config and fails when installed state drifts from it. |

### Trust boundaries

- The KVM host, its administrators, and users with libvirt control are trusted.
  They can inspect or control worlds.
- The deployment is for one developer or a trusted team, not hostile tenants.
- Repository and devcontainer code is trusted inside its world and can access
  that world's checkout.
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
