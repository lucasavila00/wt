# Database

WT uses SQLite through Diesel at `~/.local/state/wt/instances.db`.

`worlds` stores each world's immutable UUID, globally unique mutable name,
creation time, status, request fingerprint, resources, reservation state, SSH
endpoint. World listings use creation order. The UUID also
names the world's disk; neither a disk identifier nor a libvirt domain name is
stored in the registry.
`agent_tool_reports` stores `wtg tools` feedback and is deleted with its world.
The registry does not store live terminal observations, rendered terminal
contents, Byobu window presentation, raw Codex events, or agent-tool
authorization state.

World insertion and capacity reservation happen in one immediate transaction.
CPU, RAM, and disk admission is therefore atomic. Stopped worlds reserve no CPU
or RAM and count only current disk allocation; starting one reacquires its full
configured capacity.

The bootstrap schema and generated Diesel schema are owned by
`crates/shared/workload-registry` and embedded in the binaries. For now there is one
bootstrap migration for a fresh registry, not an upgrade/backfill chain. Clear the
development server state with `make clear` before installing this schema; existing
databases are not supported across this reset. Normal process and guest restarts still
retain their database and durable recovery state.
