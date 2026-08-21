# Database

WT uses SQLite through Diesel at `~/.local/state/wt/instances.db`.

`guests` stores state shared by every kind: identity, kind, backend, disk, and
resource reservation. `worlds` stores retained ownership, name, status,
fingerprint, and guest SSH. `devcontainers` stores repository, Git grant, and
app SSH. `hosts` stores the host Git grant. `runners` stores GitHub CI lifecycle
state. `agent_git_reports` stores `wt-git-hosting` feedback attributed to the
authenticated world and removes it with that world.

The store rejects an unknown kind, a missing required subtype, or a subtype on
the wrong kind.

`disks` records the independent qcow2 disk owned by each guest.

Capacity changes and guest insertion happen in one immediate transaction. CPU,
RAM, and disk reservations therefore apply atomically across devcontainer,
host, and GitHub CI worlds when both services use the same registry path. The
CI operator must open that same registry path and capacity configuration.
Stopped retained worlds reserve no CPU or RAM and count only their disk file's
current use. Starting one reacquires its full configured capacity.

## Schema changes

Migrations and generated schema are owned by `crates/shared/workload-registry`. Migrations
are embedded in the binaries. Normally, add a migration there and commit its
generated schema with it.

ADR 0026 replaces the initial schema in place. There is no migration from the
older database: run `make nuke` before installing this version. The wire
protocol is version 2.
