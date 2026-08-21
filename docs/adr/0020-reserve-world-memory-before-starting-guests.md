# ADR 0020: Reserve world memory before starting guests

- Status: Accepted
- Date: 2026-08-13
- Amended by: [ADR 0045](0045-stop-retained-worlds.md)

## Context

WT lets libvirt start more guest memory than the host can sustain.

We hit this on a 31 GiB host after creating five 8 GiB worlds. The kernel OOM
killer killed `mt3`'s QEMU process, libvirt reported `reason=crashed`, and
`wt ls` only said that the machine was stopped. We captured the host logs and
then removed `mt3`, so its writable disk is gone.

We need to reject unsafe creates before starting QEMU and give users a WT
command for recovering a stopped world.

## Decision

At startup, `wt-server` reads the host's total physical RAM and uses it as the
aggregate world-memory limit. This is automatic and has no configuration.
Read total RAM, not currently free RAM: free memory changes constantly and is
not a stable capacity contract.

Every retained world reserves its configured memory, including provisioning,
stopped, destroying, and error worlds. Only a successful `wt rm` releases it.
The server checks and inserts the reservation in one SQLite write transaction,
so concurrent creates cannot both claim the same capacity.

When capacity is full, the server returns a typed capacity error with total host
RAM, reserved memory, and requested memory. It creates no world or disk.
`wt new` then says what is full and prompts:

```text
Free capacity in another terminal with `wt ls` and `wt rm CONTEXT.WORLD`.
Press Enter to retry or Esc to cancel.
```

Retry sends the same confirmed request. If one world is larger than host RAM,
fail normally and ask the user to choose less RAM instead of offering a retry.

Represent a stopped guest as `stopped`, not generic `error`. Add
`wt start NAME` to start its existing disk and reconcile it back to `setup` or
`running`. `wt ls` shows the provider reason when available and suggests the
qualified `wt start` and `wt rm` commands.

Starting the domain alone does not recover its stopped devcontainer. The
container, Compose-sidecar, bounded-readiness, and app-SSH recovery behavior is
defined by
[ADR 0025](0025-recover-world-containers-after-guest-start.md).

Do not restart stopped guests automatically. The condition that stopped the
guest may still exist. Do not suggest `virsh`; recovery must work through WT for
local, remote, and future providers.

## Consequences

- WT rejects aggregate memory oversubscription before creating a guest.
- Users can free capacity in another terminal and retry without re-entering the
  world configuration.
- Stopped worlds keep their memory reservation until started or removed.
- Starting a stopped world restores its retained containers and verifies the
  primary devcontainer before returning it to `running`.
- No server capacity setting is required.
- Total RAM does not reserve headroom for the host, QEMU overhead, or unrelated
  processes, so this prevents WT allocation oversubscription but cannot prevent
  every host OOM.
