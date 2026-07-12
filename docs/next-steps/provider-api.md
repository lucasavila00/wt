# Provider API split

Split machine lifecycle from world provisioning before adding another provider.
This pass is a refactor of the current libvirt backend: it must preserve the
server API, configuration file, SQLite schema, provisioning logs, SSH inventory,
and user-visible create, inspect, and delete behavior.

## Boundary

Add a `wt-provider` crate for provider-neutral contracts and shared world
provisioning. The dependency direction is:

```text
wt-server -> wt-provider <- wt-libvirt
```

`wt-provider` must not depend on `virt`, libvirt domain types, QEMU guest-agent
JSON, or host filesystem paths used to build a libvirt domain. `wt-libvirt`
keeps image, domain, network, cloud-init seed, and per-machine file lifecycle.
The server constructs the libvirt provider and shared provisioner explicitly;
there is still one fixed backend per server.

Keep a provider-neutral composite world-lifecycle interface at the server
boundary so `wt-server` and its injected service tests do not need to orchestrate
provider internals. Git-passphrase validation belongs to the provisioner side of
that composite because it owns the Git identity.

## Machine contract

A provider returns a reachable Ubuntu 24.04 amd64 machine with a working
privileged command and file transport. The provider owns:

- Machine creation, lookup, and deletion.
- Its resource ID and provider-specific resource names.
- CPU, memory, disk, image selection, and network discovery.
- Bootstrapping whatever is required to make its transport available. For
  libvirt this includes cloud-init and the QEMU guest agent.

The provider does not know about Git, devcontainers, WT guest helpers, the app
SSH server, or repository setup. The golden image may accelerate provider and
world setup, but correctness after the transport becomes available must not
depend on tools preinstalled in that image.

Use provider-neutral equivalents of these operations; exact Rust ownership can
follow the simplest object-safe or generic design:

```text
MachineProvider::create(MachineSpec) -> Machine
MachineProvider::inspect(provider_id) -> Option<Machine>
MachineProvider::delete(provider_id)
```

`MachineSpec` includes the stable provider ID selected before creation and the
requested CPU, memory, and disk. In this pass the provider ID remains the
existing `backend_id`, so neither the registry schema nor its values change.
`Machine` contains that ID, current network information, and a
`GuestTransport`; it contains no provisioned-world state.

The lifecycle operations have explicit semantics:

- `create` succeeds only after the machine is running and its transport is
  usable. If it fails after allocating anything, it attempts to remove only
  resources created for that provider ID. A cleanup failure is reported as
  secondary context without hiding the create error.
- `inspect` returns `None` only when the provider resource does not exist.
  Unreachable, malformed, or partially present resources return an error. It
  refreshes mutable network information without modifying the guest.
- `delete` is safe to retry and succeeds when the resource is already absent.
  It removes only resources owned by the provider ID. Partial cleanup is an
  error and a later delete retries the remaining work.

These rules preserve recovery after daemon interruption: the stored provider ID
is sufficient for `wt rm` even when world provisioning never completed.

## Guest transport

`GuestTransport` is synchronous and supports the operations the existing
provisioner uses:

- Run an executable with an argument vector, optional standard input, an
  absolute deadline, and incremental combined output written to a supplied
  sink. Return the exit status and a bounded output tail for diagnostics.
- Capture stdout and stderr separately with caller-specified byte limits. Treat
  exceeding a limit as an error rather than allocating without bound.
- Replace a file with supplied bytes, owner, group, and mode. Do not require a
  shell to transfer file contents.

Transport errors, deadline expiry, nonzero command exit, and destination-write
errors remain distinguishable enough for the provisioner to add phase context.
The transport never logs command input or file contents because they can contain
the Git passphrase or private key. Provider implementations may add stricter
constraints, but shared provisioning must not accept or expose a libvirt domain.

Libvirt implements this contract with the QEMU guest agent. Preserve the
current polling backoff, 64-KiB diagnostic tail, incremental log flushing, file
chunking below the guest-agent message limit, and best-effort removal of
temporary command-output files. A later static SSH provider will implement the
same contract with pinned OpenSSH; it is not part of this pass.

This provider-specific transport choice refines the common-SSH suggestion in
[`more-providers.md`](./more-providers.md): the shared provisioner depends on
transport behavior, not on SSH or a libvirt domain. Converting the working
libvirt provisioning path from the QEMU guest agent to SSH is not required to
prove that boundary.

## World provisioner

`WorldProvisioner` receives a `Machine`, the current provider-neutral provision
specification, and the provisioning log sink. It owns everything inside the
machine:

- Verify Ubuntu 24.04 amd64 and privileged execution before changing the guest.
- Install missing required packages and verify Docker Engine, Buildx, Compose,
  Git, OpenSSH, CA support, Dev Container CLI, and the configured tmux or Byobu
  frontend. Re-running these checks must be safe when the golden image already
  contains the tools.
- Create or verify the `wt` user and `/workspace` with the required ownership.
- Configure authorized keys and registry-cache CA and Docker trust.
- Clone the repository and configure checkout-local Git credentials and the
  optional author name and email.
- Start the stock devcontainer, install WT helpers, and configure app SSH.
- Verify the guest SSH endpoint against the guest host keys read through the
  transport, and verify app SSH before returning access data.

Provisioning returns the existing provider-neutral `World` value: current guest
address, guest SSH access, and app SSH access. Inspection takes a `Machine`
returned by the provider, reads WT and SSH state through its transport without
repairing it, and returns the same `World` value. Missing or changed guest/app
host identity remains an error; a changed DHCP address with the same pinned
identity updates the stored address as it does today.

Keep phase names and log ordering stable where responsibilities only move. Never
write the Git passphrase, private key contents, or other secrets to the
provisioning log or an error.

## Composition and failure handling

The composite lifecycle used by `wt-server` performs create as:

```text
reserve world with stable provider ID
  -> MachineProvider::create
  -> WorldProvisioner::provision
  -> store running world
```

If world provisioning fails, retain that error as the world's `last_error`, then
ask the provider to delete the machine. Append a cleanup failure to the durable
provisioning log, but do not replace or obscure the provisioning error. The
errored registry row and stable provider ID remain so `wt rm` can retry provider
cleanup.

For reconciliation of a running world:

```text
MachineProvider::inspect
  -> None: mark the world missing/error
  -> Machine: WorldProvisioner::inspect
  -> preserve host-key checks and refresh mutable address data
```

Delete delegates to the provider in this pass because deleting the libvirt
machine removes the entire world. The later static SSH provider will require a
separate provisioner cleanup operation before releasing its claim; do not add
that behavior speculatively here.

## Implementation order

1. Add `wt-provider` with provider-neutral lifecycle values, errors, transport
   contracts, and the existing server-facing composite world interface. Move
   only configuration and provisioning types that would otherwise create a
   dependency from `wt-provider` back to `wt-libvirt`; preserve the current TOML
   shape.
2. Adapt the QEMU guest-agent command, capture, and file operations to
   `GuestTransport`, with fake-transport unit tests for deadlines, bounded
   capture, streaming failures, exit diagnostics, and file metadata.
3. Extract guest setup, Git, devcontainer, helper, and SSH verification into
   `WorldProvisioner`. Replace direct `Domain` use in that code with the
   transport and add tests over a recording fake transport.
4. Reduce `wt-libvirt` to `MachineProvider` plus its transport implementation.
   Keep only image, cloud-init bootstrap, domain, network, and per-machine file
   lifecycle there.
5. Compose the libvirt provider and shared provisioner in `wt-server`. Preserve
   the current API/config/schema and adapt the existing injected worker tests at
   the composite boundary.
6. Add failure-path tests proving partial create cleanup, provisioning cleanup,
   cleanup-error precedence, missing-machine inspection, host-identity mismatch,
   idempotent delete, and retry after partial deletion.

## Completion criteria

- `wt-provider` contains no libvirt/QEMU types, and `wt-libvirt` contains no Git,
  devcontainer, app-helper, or app-SSH provisioning logic.
- Starting from a supported Ubuntu machine whose provider transport is usable,
  provisioning succeeds even when Docker, Git, OpenSSH, the Dev Container CLI,
  and session tools were not supplied by the golden-image recipe.
- Existing server service tests still cover reservation, asynchronous jobs,
  restart reconciliation, list/inspect, error recording, and delete behavior.
- Unit tests cover the provider, transport, provisioner, and composite failure
  contracts above, using complete Insta snapshots for stable multiline logs and
  diagnostics.
- Run `cargo fmt --all --check`, tests and Clippy for every affected crate, and
  the existing libvirt KVM end-to-end test because this refactor changes the
  real machine and provisioning path. Review snapshots and leave no
  `.snap.new` or `.pending-snap` files.

## Not in this pass

Do not add static SSH, cloud providers, proxy commands, backend selection,
per-world provider configuration, registry schema changes, server-setup
changes, runtime overrides, or emulation fallback.
