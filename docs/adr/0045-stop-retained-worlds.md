# ADR 0045: Stop retained worlds

- Status: Proposed
- Date: 2026-08-20
- Amends: [ADR 0020](0020-reserve-world-memory-before-starting-guests.md),
  [ADR 0024](0024-use-a-shared-guest-registry.md)

## Context

An agent world does not need to keep running after the agent finishes. We only
need its durable state, such as the checkout, commits, logs, and session files.

WT can restart a world after it stops, but it cannot stop one intentionally.
Stopped worlds also keep reserving CPU and memory, so they do not free capacity
for another agent.

## Decision

Add `wt stop NAME` for retained devcontainer and host worlds.

WT asks the guest to shut down cleanly and waits for libvirt to confirm that it
has stopped. A timeout returns an error; it does not forcibly power off the
guest.

A stopped world keeps its disk, machine definition, metadata, SSH identities,
Git grant, and shared-folder data. It does not keep RAM or live processes. QEMU,
guest networking, SSH, and virtiofs processes stop with the guest.

Stopped worlds continue to reserve disk capacity, but release their CPU and
memory reservations after shutdown is confirmed. `wt start` atomically reserves
CPU and memory before booting the world. If capacity is unavailable, the world
stays stopped.

Use the existing `stopped` state and record that the stop was requested. Do not
add a `paused` state.

`wt start` boots the existing disk. Devcontainer worlds use the existing
container recovery path. Exact processes and previously running containers are
not preserved.

An agent controller may call `wt stop` after it has saved the agent's final
result. WT does not treat an SSH disconnect or agent process exit as task
completion.

## Consequences

- Idle worlds use storage but no guest CPU or memory.
- More worlds can be retained than can run at once.
- Starting a stopped world can fail when compute capacity is full.
- Suspension and managed save are unnecessary because WT does not preserve RAM
  or live process state.
