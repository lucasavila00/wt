# Database

WT uses SQLite through Diesel at `~/.local/state/wt/instances.db`.

`worlds` stores each world's immutable UUID, globally unique mutable name,
creation time, status, request fingerprint, resources, reservation state, SSH
endpoint, and gateway grant. World listings use creation order. The UUID also
names the world's disk; neither a disk identifier nor a libvirt domain name is
stored in the registry.
`agent_tool_reports` stores `wtg tools` feedback. `pane_observations` stores
the latest fingerprint and change/freshness timestamps for each observed Byobu
pane. Both are deleted with their world. It never stores rendered terminal
contents, Codex lifecycle events, or checkout state.

World insertion and capacity reservation happen in one immediate transaction.
CPU, RAM, and disk admission is therefore atomic. Stopped worlds reserve no CPU
or RAM and count only current disk allocation; starting one reacquires its full
configured capacity.

Migrations and generated schema are owned by
`crates/shared/workload-registry` and embedded in the binaries.
