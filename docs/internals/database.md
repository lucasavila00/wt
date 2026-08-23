# Database

WT uses SQLite through Diesel at `~/.local/state/wt/instances.db`.

`worlds` stores world ownership, name, creation time, status, request
fingerprint, backend, disk, resources, reservation state, SSH endpoint, and
gateway grant. World listings use creation order.
`agent_tool_reports` stores `wt-tools` feedback and
`codex_session_reports` stores the latest per-world Codex observations. Both
are deleted with their world.

`codex_checkout_state` independently stores the latest Git context for an
active Codex session, working directory, pane, and generation. It links to the
central `repositories` catalog only when the selected remote resolves to a
configured target. The guest relay updates checkout state without changing
lifecycle ordering or activity age; unavailable or unconfigured remotes remain
stored as unjoined checkout state.

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
