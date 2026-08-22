# Expand Codex session data for client-only features

WT now has a rebuildable typed SQL summary index and canonical rollout JSONL.
It is not holistic VS Code session parity: it lacks raw history retrieval,
request/response terminal state and timing, approvals, per-file diff summaries,
and client-owned pin/archive/read/group state.

Keep SQLite for WT-owned lifecycle and rebuildable query metadata. For complete
history, have the trusted client SSH directly to the selected world and read
canonical rollout records; do not add a `wt-server` history proxy or copy
arbitrary Codex JSON into SQLite. Add new typed summary fields only with a
concrete client use; store user presentation state separately from the catalog.
