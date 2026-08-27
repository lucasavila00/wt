# Disable automatic cross-world Codex session synchronization

Disable automatic cross-world Codex session synchronization for now. Disable
more than the WT wrapper: stop mounting the global history tree directly at
each world's `~/.codex/sessions`, because upstream Codex backfills its local
state database from every rollout visible there even when
`IGNORE_CODEX_WT_CHECKS=true` skips WT reconciliation.

On `gentle-falcon`, the global virtiofs mount contained 459 rollout files
totaling about 1.05 GB. The local database reported
`backfill_state.status = running` with 351 threads indexed, but no Codex or
backfill process was alive. WT waits 30 seconds for the app-server, kills it on
timeout, and leaves Codex's backfill lease marked as running. Other Codex
processes then wait for that stale lease and fail. The database is incomplete,
not necessarily damaged as the generic Codex diagnostic claims.

Increasing the timeout only lengthens an unbounded startup operation. Sharing
the SQLite database is also unsafe, and background full scans in every world
previously caused repeated work while active sessions changed.

Keep authentication sharing unchanged. Give each world its own server-backed
sessions directory, keyed by world identity, so sessions remain durable without
every world indexing the complete history. Preserve the existing global
directory as an archive. Later, add a bounded explicit import that selects one
archived thread, projects its rollout into the destination world's sessions,
and runs `codex resume <id>`.

For recovery after changing the mount, stop Codex and move
`state_5.sqlite`, `state_5.sqlite-wal`, and `state_5.sqlite-shm` aside together.
Deleting only the main database while the global rollout tree remains visible
starts the same backfill failure again.
