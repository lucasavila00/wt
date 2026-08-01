# ADR 0015: Fork worlds with copy-on-write disks

- Status: Accepted
- Date: 2026-07-31

## Context

Creating a usable world takes about 95 seconds even when another world already
contains the same checkout, tools, Docker state, and caches.

## Decision

Add:

```text
wt fork SOURCE NEW
```

`SOURCE` and `NEW` may each be qualified as `context.world` or unqualified.
An unqualified `NEW` uses the resolved source context. Both names must resolve
to the same context because the copy-on-write graph is local to one WT server;
cross-context forks fail before contacting either server. `NEW` must not collide
with an existing world owned by the caller.

Store world disks as a copy-on-write graph. Each world has a writable head;
immutable disk nodes may be shared by several worlds.

To fork a running world:

1. Quiesce its filesystems through the QEMU guest agent.
2. Atomically pivot the source to a new head and immediately thaw it.
3. Create the fork's head from the same immutable disk point.
4. Boot the fork without network access, replace its machine and SSH
   identities, then enable networking and verify it.

The source VM and its processes keep running. Its writes pause only during the
disk pivot. The fork receives disk state, including uncommitted files, Docker
state, volumes, tools, and caches, but not RAM or running processes.

The registry owns disk references. Removing a world deletes its head and
garbage-collects immutable nodes only when nothing references them.

Fail rather than stopping the source, taking an unquiesced snapshot, or falling
back to `wt new`.

## Verification

- The source remains running and is always thawed.
- Source and fork changes are isolated after the fork point.
- Guest and app SSH identities differ.
- Removing either world does not break the other.
- Unreferenced disk nodes are removed.
- Interrupted forks leave the source usable.
- Forking is substantially faster than `wt new`.

## Consequences

Forking should take roughly the time needed to boot and verify an existing
guest instead of rebuilding it.

WT gains a disk graph and garbage collection.

## Alternatives

Stopping the source was rejected because users often fork when it is busy.
Copying RAM was rejected because it duplicates live connections, credentials,
and process state.
