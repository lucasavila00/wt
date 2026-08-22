# Expand Codex session data for client-only features

WT now has a rebuildable typed SQL summary index and canonical rollout JSONL.
It is not holistic VS Code session parity: it lacks raw history retrieval,
request/response terminal state and timing, approvals, per-file diff summaries,
and client-owned pin/archive/read/group state.

Keep SQLite for WT-owned lifecycle and rebuildable query metadata. For complete
history, add an on-demand API that streams canonical rollout records to trusted
clients instead of copying arbitrary Codex JSON into SQLite. Add new typed
summary fields only with a concrete client use; store user presentation state
separately from the catalog.
