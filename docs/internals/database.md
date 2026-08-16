# Database

WT uses SQLite through Diesel at `~/.local/state/wt/instances.db`.

`guests` stores state shared by every kind: identity, kind, backend, head disk,
and resource reservation. `worlds` stores retained ownership, name, status,
fingerprint, and guest SSH. `devcontainers` stores repository, Git grant, and
app SSH. `runners` stores GitHub CI lifecycle state. Host worlds need no subtype
row. `agent_git_reports` stores `ag-git` feedback attributed to the authenticated
world and removes it with that world.

The store rejects an unknown kind, a missing required subtype, or a subtype on
the wrong kind.

`disk_nodes` records qcow2 parentage. Shared parents become immutable. Deletion
computes unreachable nodes from leaf to root and removes only those disks.

Capacity changes and guest insertion happen in one immediate transaction. CPU,
RAM, and disk reservations therefore apply atomically across devcontainer,
host, and GitHub CI worlds when both services use the same registry path. The
CI operator must open that same registry path and capacity configuration.

## Schema changes

Migrations and generated schema are owned by `crates/wt-registry`. Migrations
are embedded in the binaries. Normally, add a migration there and commit its
generated schema with it.

ADR 0026 replaces the initial schema in place. There is no migration from the
older database: run `make nuke` before installing this version. The wire
protocol remains version 1.
