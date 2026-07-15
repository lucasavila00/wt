# Provider architecture

`wt-provider` separates machine management from world provisioning.
`wt-libvirt` supplies the current machine implementation.

```text
wt-server -> wt-provider <- wt-libvirt
wt-server-setup -> wt-provider + wt-libvirt
```

## Ownership

`wt-provider` owns:

- provider-neutral types and lifecycle;
- guest command and file transport;
- OS bootstrap and world provisioning;
- bootstrap package and version policy.

`wt-libvirt` owns:

- libvirt domains, images, disks, networks, and host files;
- QEMU guest-agent transport;
- machine creation, inspection, and deletion.

Provider-neutral code contains no libvirt or QEMU types. Libvirt code contains no
Git, devcontainer, registry-cache, or app-SSH provisioning.

## Machine provider

```text
create(MachineSpec, progress) -> Machine
inspect(provider_id) -> Option<Machine>
delete(provider_id)
```

`MachineSpec` contains the stable provider ID and requested CPU, memory, and
disk. `Machine` contains the provider ID, current network data, and a
`GuestTransport`.

- `create` returns when the machine and transport are ready. On failure, it
  attempts to remove partial resources without hiding the original error.
- `inspect` returns `None` only when no provider resource exists. It refreshes
  network data without changing the guest.
- `delete` is idempotent and attempts independent cleanup after errors.
- The stored provider ID is sufficient to retry deletion after interruption.

## Guest transport

The synchronous transport can:

- run a command with a deadline and streamed output;
- capture bounded stdout and stderr;
- write a file and set its ownership and mode.

It distinguishes transport, deadline, output-limit, exit-status, and log-sink
errors. Output limits are enforced while reading. Command input and file
contents are never included in logs or errors.

The libvirt implementation uses the QEMU guest agent. Shared provisioning uses
only `GuestTransport`, never a libvirt domain.

## World provisioner

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
inspect: machine.inspect -> provisioner.inspect -> World
delete:  machine.delete
```

Machine creation cleans up its own partial resources. Provisioning failure is
recorded as the primary error before machine deletion is attempted. Cleanup
errors are logged as secondary context, and the errored world remains available
for a later `wt rm` retry.
