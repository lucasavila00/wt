# ADR 0023: Use a shared guest registry

- Status: Accepted
- Date: 2026-08-14
- Amends: [ADR 0020](0020-reserve-world-memory-before-starting-guests.md)

## Context

The current registry stores development worlds directly in `instances`. Its
memory sum is enough while `wt-server` is the only guest creator.

`wt-runner` will create guests in another process. Separate world and runner
capacity tables could both admit the last host capacity. A standalone capacity
row would also split a guest's reservation from the libvirt machine and disk
whose lifetime it represents.

## Decision

Store every retained KVM guest in one `guests` table. A guest row owns:

- its kind: `world` or `runner`;
- its libvirt and head-disk identities; and
- its CPU, memory, and disk reservation.

Keep lifecycle-specific data in one-to-one subtype tables:

- `worlds` stores development status, Git, and SSH data;
- `runners` stores CI status and GitHub runner identity; and
- `disk_nodes` remains the shared copy-on-write disk graph.

Creating a world or runner inserts its disk node, guest, and subtype in one
immediate SQLite transaction. Admission sums `guests` in that transaction, so
`wt-server` and `wt-runner` cannot over-admit each other.

`wt-server-setup` installs one strict host-capacity configuration containing the
CPU, memory, and disk limits. Both services read that file and pass the same
limits to the registry. Do not derive admission from currently free resources;
they change while a request is running.

Capacity errors identify whether CPU, memory, or disk is full and report its
limit, reserved amount, and request. This replaces the memory-only wire shape
and bumps the WT protocol version.

Every retained guest reserves its configured resources regardless of lifecycle
state. Delete the guest row only after its libvirt machine and disposable disks
have been removed. Failed cleanup keeps the row and its reservation for later
reconciliation.

Put the schema and common admission code in `wt-registry`. World behavior stays
in `wt-server`; runner behavior stays in `wt-runner`.

Do not migrate the old `instances` table. Before installing this schema, the
operator must run `make nuke` as the server user and accept that it destroys all
existing WT guests and state. Then install WT again from scratch.

## Consequences

- Worlds and runners share one atomic host-capacity boundary.
- A reservation has the same lifetime as the guest it protects.
- World and runner lifecycle fields remain separate.
- Existing WT installations require a destructive reset and reinstall.
