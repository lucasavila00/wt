# Database

WT uses SQLite through Diesel at `~/.local/state/wt/instances.db`.

`worlds` stores retained ownership, name, creation time, status, request
fingerprint, backend, disk, resources, reservation state, SSH endpoint, and
gateway grant. World listings use creation order.
`agent_tool_reports` stores `wt-tools` feedback and
`codex_session_reports` stores the latest per-world Codex observations. Both
are deleted with their world.

World insertion and capacity reservation happen in one immediate transaction.
CPU, RAM, and disk admission is therefore atomic. Stopped worlds reserve no CPU
or RAM and count only current disk allocation; starting one reacquires its full
configured capacity.

Migrations and generated schema are owned by
`crates/shared/workload-registry` and embedded in the binaries.
