# ADR 0064: Cache Codex session metadata

- Status: Accepted; Date: 2026-08-22

## Decision

- Maintain a rebuildable Codex session catalog in the server registry.
- Treat rollout JSONL files as the canonical session history.
- Discover rollouts in the shared sessions tree and incrementally parse appended
  complete records from a persisted byte offset.
- Rebuild an entry when its rollout path changes, shrinks, or predates its
  cached modification time.
- Remove catalog entries whose rollout no longer exists.
- Refresh the catalog periodically in `wt-server` and before serving a session
  inventory request.
- Store bounded title, latest user-message, and latest agent-message previews;
  their event timestamps; session creation and rollout modification times;
  working directory; model and Codex CLI version; turn, command, and file-change
  counts; token totals; rollout path, length, and parsing offset.
- Return the broad, typed metadata through the control protocol; WT clients are
  trusted to view Codex data available to their worlds.
- Keep raw rollout records in their canonical JSONL files rather than duplicate
  them in SQLite. A future history API may stream those records on demand.
- Do not make arbitrary upstream JSON a SQLite schema or a stable protocol
  contract; add typed fields when clients have a concrete use for them.
- Normalize previews as untrusted terminal text and limit each to 640 UTF-8
  bytes.
- Skip subagent rollouts and isolate malformed rollout records to their entry.

## Consequences

- Client-only releases can present richer session summaries without changing
  server extraction.
- After warm-up, parsing cost is proportional to new rollout data; discovery
  still visits every rollout path and is tracked for follow-up optimization.
- The catalog can be deleted and reconstructed from the sessions tree.
- SQLite contains bounded excerpts of the latest user and agent messages.
