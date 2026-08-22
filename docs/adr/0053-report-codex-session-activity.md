# ADR 0053: Capture Codex session activity per WT context

- Status: Proposed; Date: 2026-08-21

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

| Codex hook | State |
| --- | --- |
| `SessionStart` | `unknown` (with its raw start source) |
| `UserPromptSubmit` | `working` |
| `Stop` | `needs_attention` |
| `SessionEnd` | `inactive` |

Every hook reports `session_id`, `cwd`, optional Git repository and branch
context, `tmux_session`, and `%N` `pane_id`.
The Byobu target is required and verified before forwarding.
WT parses known session-start sources while preserving the raw value. The shell
renders that value with an unknown state, for example `unknown(compact)`.

Use the existing authenticated guest relay and vsock path. Its grant supplies
`world_id`; the hook cannot choose it. Hooks are short-timeout and fail-open.

Store the latest observation per `(world_id, session_id)`:

```text
session_id, world_id, cwd, state,
session_start_source, tmux_session, pane_id, received_at_unix_ms
```

Receipt time is required because hooks provide no heartbeat. World deletion
cascades to its observations.

### API

```text
session { session_id, title?, rollout_updated_at_unix_ms?, observations[] }
observation { world_id, world_name, cwd, repository_root?, repository_url?,
              git_branch?, state, received_at_unix_ms,
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
