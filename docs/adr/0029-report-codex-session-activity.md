# ADR 0029: Capture Codex session activity per WT context

- Status: Accepted; Date: 2026-08-21

## Problem

WT cannot query Codex for a session inventory. Codex instead keeps one on-disk
event log for each conversation. Codex calls this log a rollout.

A rollout is a JSON Lines file: one JSON record per line. Its first record,
`session_meta`, identifies the session and whether it belongs to a subagent.
Later records append prompts, responses, and tool events. The file remains after
Codex exits, so WT uses rollouts as its durable session catalog.

A rollout does not identify the WT world or Byobu pane running Codex. Its event
stream is also not a reliable current activity signal.

## Decision

Each `wt-server` combines a rollout catalog with lifecycle observations.

### Rollout catalog

- Read `session_meta` for the canonical session UUID and source.
- Exclude subagent rollouts.
- Expose the file modification time; ignore later records.

### Lifecycle observations

Codex hooks are commands invoked by Codex at configured lifecycle events. WT
installs hooks for these events:

| Codex hook | Lifecycle state | Compaction phase |
| --- | --- | --- |
| `SessionStart` | `unknown` (with its raw start source) | clear, except `compact` starts compaction |
| `PreCompact` | unchanged | start |
| `PostCompact` | unchanged | clear |
| `UserPromptSubmit` | `working` | clear |
| `Stop` | `needs_attention` | clear |
| `SessionEnd` | `inactive` | clear |

Every hook reports only `session_id`, `cwd`, `tmux_session`, `%N` `pane_id`,
and a per-pane generation and sequence. The Byobu target is required and
verified before forwarding. The relay independently discovers and reports
checkout state.
WT parses known session-start sources while preserving the raw value. The shell
renders that value with an unknown state, for example `unknown(startup)`.
Compaction is a transient phase, not a lifecycle state: it preserves the
previous lifecycle state and the shell adds a `COMPACTING` indicator until
`PostCompact` clears it.

The guest assigns the sequence under a per-pane file lock before it sends the
report. A new session gets a new generation; later
events from an older session retain their original generation. The registry
accepts only a lexicographically newer `(generation, sequence)` for a pane, so
delayed or duplicate old events cannot overwrite its replacement.

Use the existing authenticated guest relay and vsock path. Its grant supplies
`world_id`; the hook cannot choose it. Hooks are short-timeout and fail-open.

Store the latest observation per `(world_id, session_id)`:

```text
session_id, world_id, cwd, state, is_compacting, pane_generation, pane_sequence,
session_start_source, tmux_session, pane_id, received_at_unix_ms
```

Receipt time is required because hooks provide no heartbeat. World deletion
cascades to its observations.

### API

```text
session { session_id, title?, rollout_updated_at_unix_ms?, observations[] }
observation { world_id, world_name, cwd, repository_root?, repository_url?,
              git_branch?, state, is_compacting, received_at_unix_ms,
              target { tmux_session, pane_id } }
```

Return rollout-only and report-only sessions. Preserve every world observation.

### Context boundary

- No cross-context database, replication, or synchronization.
- Context names are client-side and are not returned by `wt-server`.
- Session identity: `(context, session_id)`.
- Live location: `(context, world_id, tmux_session, pane_id)`.
- Never discard duplicate session UUIDs or locations across contexts.

`wt shell` aggregation and interaction are out of scope.
