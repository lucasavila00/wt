# Provider API split

Split machine creation from world setup before adding another provider.

## Rules

- A provider returns a reachable Ubuntu 24.04 amd64 machine with a privileged
  command and file transport.
- The provider owns machine create, inspect, and delete.
- The provider does not know about Git, devcontainers, WT helpers, or app SSH.
- The world provisioner owns everything inside the machine.
- The world provisioner installs or verifies every required tool.
- The golden image is a cache. Correctness must not depend on its contents.

## Interfaces

`MachineProvider`:

- `create(MachineSpec) -> Machine`
- `inspect(provider_id) -> Option<Machine>`
- `delete(provider_id)`

`Machine` contains the provider ID, network information, and a
`GuestTransport`.

`GuestTransport` can:

- Run a command with arguments, input, deadline, and streamed output.
- Capture bounded command output.
- Write a file with ownership and permissions.

Libvirt implements this transport with the QEMU guest agent. A later static SSH
provider implements it with pinned OpenSSH.

`WorldProvisioner`:

- Verifies the OS and privileged access.
- Installs or verifies Docker, Buildx, Compose, Git, OpenSSH, the Dev Container
  CLI, CA support, and tmux or Byobu.
- Creates the `wt` user and `/workspace`.
- Configures authorized keys and registry-cache trust.
- Clones the repository and configures Git credentials and author values.
- Starts the devcontainer.
- Installs WT helpers.
- Returns verified guest and app SSH access.

## Flow

```text
wt-server
  -> MachineProvider::create
  -> WorldProvisioner::provision
  -> store running world
```

If world provisioning fails, keep its error and ask the provider to delete the
machine. Log deletion errors without replacing the original error.

Inspection opens the machine through the provider, then inspects WT state
through the world provisioner.

## First pass

1. Add the provider, transport, and provisioner interfaces outside
   `wt-libvirt`.
2. Put the QEMU guest-agent operations behind `GuestTransport`.
3. Leave only image, domain, network, and machine-file lifecycle in
   `wt-libvirt`.
4. Move tool setup, Git, devcontainer, helpers, and SSH setup into the world
   provisioner.
5. Compose both parts in `wt-server` without adding backend selection.
6. Verify current libvirt create, inspect, failure cleanup, and delete behavior.

Do not add static SSH, proxy commands, config branching, or server setup changes
in this pass.
