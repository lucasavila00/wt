# Avoid scanning every Codex rollout on every refresh

The catalog worker now owns normal refreshes, inventory reads the SQLite
snapshot, and unchanged entries are not rewritten. It still recursively walks
every rollout path every two seconds to discover changes.

Replace polling with a bounded filesystem-notification or debounce design while
preserving startup warm-up and recovery after missed notifications.
