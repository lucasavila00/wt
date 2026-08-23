# Database

WT uses SQLite through Diesel at `~/.local/state/wt/instances.db`.

`worlds` stores retained ownership, name, creation time, status, request
fingerprint, backend, disk, resources, reservation state, SSH endpoint, and
gateway grant. World listings use creation order.
`agent_tool_reports` stores `wt-tools` feedback and
`codex_session_reports` stores the latest per-world Codex observations. Both
are deleted with their world.

Each Codex observation keeps lifecycle receipt time separate from Git-context
health. The guest relay updates repository metadata, Git check time, and a
sanitized Git error only when the exact active session, working directory, and
pane generation still match. A Git update never changes lifecycle ordering or
activity age.

`codex_session_catalog` is a rebuildable index of the shared Codex rollout
tree. It stores bounded session summaries, aggregate activity and token counts,
and the byte offset needed to parse only newly appended rollout records. The
rollout JSONL files remain the canonical session history.

World insertion and capacity reservation happen in one immediate transaction.
CPU, RAM, and disk admission is therefore atomic. Stopped worlds reserve no CPU
or RAM and count only current disk allocation; starting one reacquires its full
configured capacity.

Migrations and generated schema are owned by
`crates/shared/workload-registry` and embedded in the binaries.
