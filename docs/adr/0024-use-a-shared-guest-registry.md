# ADR 0024: Use a shared guest registry

- Status: Accepted
- Date: 2026-08-14
- Amends: [ADR 0020](0020-reserve-world-memory-before-starting-guests.md)
- Amended by: [ADR 0026](0026-make-world-kinds-first-class.md)

## Context

The current registry stores development worlds directly in `instances`. Its
memory sum is enough while `wt-server` is the only guest creator.

`wt-runner` will create guests in another process. Separate world and runner
capacity tables could both admit the last host capacity. A standalone capacity
row would also split a guest's reservation from the libvirt machine and disk
whose lifetime it represents.

## Decision

Store every managed application guest in one `guests` table. Temporary image
build guests are not application state. A guest row owns:

- its kind: `devcontainer`, `host`, or `github-ci`;
- its libvirt and head-disk identities; and
- its CPU, memory, and disk reservation.

Keep lifecycle-specific data in one-to-one subtype tables:

- `worlds` stores retained ownership, name, status, fingerprint, and guest SSH;
- `devcontainers` stores repository, Git grant, and app SSH data;
- host worlds need no subtype beyond `worlds`;
- `runners` stores CI status and GitHub runner identity; and
- `disk_nodes` remains the shared copy-on-write disk graph.

Creation inserts the disk node, guest, and required subtype rows in one
immediate SQLite transaction. Admission sums `guests` in that transaction, so
separate owners cannot over-admit each other.

`wt-server-setup` installs one strict CPU, memory, and disk capacity file.
`wt-server` reads it; the future runner must read the same file. Do not derive
admission from currently free resources because they change during a request.

Capacity errors identify whether CPU, memory, or disk is full and report its
limit, reserved amount, and request. This replaces the memory-only wire shape.
Keep protocol version 1: installation requires a full reset, and ADR 0007's
exact client/server commit check already rejects mismatched wire types.

Every retained guest reserves its configured resources regardless of lifecycle
state. Delete the guest row only after its libvirt machine and disposable disks
have been removed. Failed cleanup keeps the row and its reservation for later
reconciliation.

Put the schema and common admission code in `wt-registry`. Retained behavior
stays in `wt-server`; CI lifecycle stays in `wt-github-ci`, operated by the
future `wt-runner` executable.

Do not migrate the old `instances` table. Before installing this schema, the
operator must run `make nuke` as the server user and accept that it destroys all
existing WT guests and state. Then install WT again from scratch.

## Consequences

- Worlds and runners share one atomic host-capacity boundary.
- A reservation has the same lifetime as the guest it protects.
- World and runner lifecycle fields remain separate.
- Existing WT installations require a destructive reset and reinstall.
