# Provider API split

Separate machine lifecycle from world provisioning.

Flow:

```text
reserve -> create machine -> provision guest -> start devcontainer
        -> verify SSH -> mark running
```

WT has not shipped. Breaking API, config, and database changes are allowed. Do
not add migrations, compatibility shims, legacy readers, or fallback paths.

This is still a code refactor. User-visible changes are not a goal. Make them
only when the provider boundary requires them.

## Ownership

```text
wt-server -> wt-provider <- wt-libvirt
wt-server-setup -> wt-provider + wt-libvirt
```

`wt-provider` owns:

- provider-neutral contracts and values;
- guest transport;
- OS bootstrap and world provisioning;
- composite lifecycle used by `wt-server`;
- bootstrap package and version policy.

`wt-libvirt` owns:

- image, domain, network, disk, seed, and host files;
- QEMU guest-agent bootstrap and transport;
- machine inspect and delete.

Rules:

- `wt-provider` has no `virt`, libvirt, QEMU JSON, or libvirt-path types.
- `wt-libvirt` has no Git, devcontainer, helper, registry-cache, or app-SSH
  provisioning.
- One fixed libvirt provider per server.
- Split runtime config into machine and provisioner config.
- Golden-image and runtime bootstrap use one package/version policy.

## Machine provider

```text
create(MachineSpec, progress) -> Machine
inspect(provider_id) -> Option<Machine>
delete(provider_id)
```

`MachineSpec`: stable provider ID, CPU, memory, disk.

`Machine`: provider ID, current network data, `GuestTransport`. No world or SSH
access state.

Contract:

- Validate IDs before resource-name or host-path use.
- `create` returns after the machine runs and transport works.
- `create` writes readiness output to the durable progress sink.
- Failed `create` attempts all cleanup. Create error stays primary. Cleanup
  errors become secondary context.
- `inspect` returns `None` only when no provider resource exists. Stopped,
  unreachable, malformed, or partial resources are errors.
- `inspect` refreshes network data without guest mutation.
- `delete` is idempotent. Attempt independent cleanup after failures. Report all
  failures. Touch only resources owned by the validated ID.
- Stored `backend_id` is enough for `wt rm` after interruption.

## Guest transport

Synchronous operations:

- Run absolute executable path, args, optional stdin, absolute deadline,
  streaming combined output, exit status, and 64-KiB diagnostic tail.
- Capture stdout and stderr separately with per-stream limits and deadline.
- Write bytes, then set owner, group, and mode.

Contract:

- Enforce capture limits while reading. Never collect unbounded output first.
- Distinguish transport, deadline, overflow, exit-status, and log-sink errors.
- Provisioner adds phase context and handles nonzero exit.
- Never log stdin or file contents.

Libvirt uses QEMU guest agent. Preserve polling backoff, incremental flushing,
64-KiB tail, sub-limit file chunks, and temporary-file cleanup. Use streamed
temporary files for bounded capture. Shared code never accepts `Domain`.

## World provisioner

Input: `Machine`, provision spec, durable log sink.

Steps:

1. Verify Ubuntu 24.04 amd64 and root or passwordless sudo.
2. Install or verify CA tools, Git, Docker, Buildx, Compose, OpenSSH, Node, Dev
   Container CLI, and tmux or Byobu.
3. Create or verify `wt` and `/workspace`.
4. Configure authorized keys, create or verify guest SSH identity, registry CA,
   and Docker proxy.
5. Clone. Configure checkout Git credentials and optional author.
6. Start stock devcontainer.
7. Install helpers. Configure app SSH.
8. Verify guest and app SSH. Return current `World`.

Bootstrap requirements:

- Works on stock supported Ubuntu after provider transport starts.
- Idempotent on the golden image and safe to retry.
- Handles apt locks.
- Uses the same sources and pinned versions as image build.

`inspect` reads without repair. Identity change is an error. DHCP change with
the same guest and app identities refreshes the address.

No passphrase, private key, stdin, or written file content in logs, errors, or
debug output.

## Composite lifecycle

`wt-provider` contains `ProvisionSpec`, `World`, the error type, and the
server-facing `WorldWorker`-shaped interface.

```text
create:    provider.create -> provisioner.provision -> World
inspect:   provider.inspect -> provisioner.inspect -> World
delete:    provider.delete
```

Failure rules:

- Machine failure: provider cleans partial resources.
- Provision failure: preserve provision error as `last_error`; call delete.
- Cleanup failure: append to durable log and secondary context. Do not replace
  `last_error`.
- Keep errored row and stable ID. `wt rm` retries cleanup.
- Missing machine and SSH identity change keep distinct error transitions.
