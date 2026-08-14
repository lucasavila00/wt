# Provider architecture

`wt-provider` defines machine management and guest transport. `wt-libvirt`
supplies the current machine implementation. `wt-devcontainer` owns
devcontainer world provisioning.

```text
wt-server -> wt-devcontainer + wt-libvirt
wt-devcontainer + wt-libvirt -> wt-provider
wt-server-setup -> wt-devcontainer + wt-libvirt
```

## Ownership

`wt-provider` owns:

- provider-neutral types and lifecycle;
- guest command and file transport.

`wt-devcontainer` owns OS bootstrap, package policy, repository checkout,
devcontainer setup, and guest/app SSH readiness.

`wt-libvirt` owns:

- libvirt domains, images, disks, networks, and host files;
- QEMU guest-agent transport;
- machine creation, inspection, and deletion.

Provider-neutral code contains no libvirt or QEMU types. Libvirt code contains no
Git, devcontainer, registry-cache, or app-SSH provisioning.

## Machine provider

```text
create(MachineSpec, progress) -> Machine
fork(ForkMachineSpec, progress) -> Machine
inspect(provider_id) -> Missing | Running(Machine) | Stopped(reason)
start(provider_id) -> Machine
delete(provider_id, garbage_disk_ids)
```

`MachineSpec` contains the stable provider and disk IDs and requested CPU,
memory, and disk. `Machine` contains the provider ID, current network data, and a
`GuestTransport`.

- `create` returns when the machine and transport are ready. On failure, it
  attempts to remove partial resources without hiding the original error.
- `fork` atomically pivots a quiesced source, boots the sibling with networking
  disabled, replaces machine and SSH identities, then enables networking.
- `inspect` distinguishes a missing, running, or stopped provider resource. It
  refreshes network data without changing the guest.
- `start` boots a stopped provider resource without replacing its disk or
  identity.
- `delete` is idempotent and attempts independent domain, machine-file, and
  registry-selected disk-node cleanup after errors.
- The stored provider ID is sufficient to retry deletion after interruption.

## Guest transport

The synchronous transport can:

- run a command with a deadline and streamed output;
- capture bounded stdout and stderr;
- write a file and set its ownership and mode.

It distinguishes transport, deadline, output-limit, exit-status, and log-sink
errors. Output limits are enforced while reading. Command input and file
contents are never included in logs or errors.

The libvirt implementation uses the QEMU guest agent. Provisioning uses only
`GuestTransport`, never a libvirt domain.

## Devcontainer provisioner

Given a `Machine`, provision specification, and output sink, the
provisioner:

1. Verifies the supported OS, architecture, and privilege level.
2. Installs the required system and development tools.
3. Configures the `wt` user, workspace, SSH, registry trust, and Docker proxy.
4. Clones the repository with temporary Git credentials, deletes them, and configures local author values.
5. Starts the devcontainer and installs WT helpers.
6. Verifies guest and app SSH and returns the current `World`.

Bootstrap is idempotent, handles apt locks, and uses the same package sources
and pinned versions as the golden-image build. Inspection reads current state
without repairing it; changed SSH identity is an error, while a changed address
with the same identity is accepted.

## Composite lifecycle

```text
create:  machine.create -> provisioner.provision -> World
fork:    machine.fork -> provisioner.inspect -> World
inspect: machine.inspect -> provisioner.inspect -> World
delete:  registry.gc -> machine.delete
```

Machine creation cleans up its own partial resources. Provisioning failure is
recorded as the primary error before machine deletion is attempted. Cleanup
errors are logged as secondary context, and the errored world remains available
for a later `wt rm` retry.
