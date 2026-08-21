# ADR 0053: Capture Codex session activity per WT context

- Status: Proposed; Date: 2026-08-21

## Problem

Codex persists each session as an append-only JSONL rollout under its sessions
directory. The first `session_meta` record contains the session UUID and source;
the remaining records contain the conversation and tool events. Subagents have
their own rollouts linked through the source metadata.

Rollouts provide the durable session catalog and modification time. They do not
identify the WT world or Byobu pane running the session, and parsing their event
stream is not a reliable activity signal.

## Decision

Each `wt-server` combines a rollout catalog with lifecycle observations.

### Rollout catalog

- Read the bounded first `session_meta` record only.
- Require a canonical session UUID.
- Exclude subagent rollouts.
- Expose the rollout modification time; infer no activity from later records.

### Lifecycle observations

| Codex hook | State |
| --- | --- |
| `SessionStart` | `unknown` |
| `UserPromptSubmit` | `working` |
| `Stop` | `needs_attention` |
| `SessionEnd` | `inactive` |

Every hook reports `session_id`, `cwd`, `tmux_session`, and `%N` `pane_id`.
The Byobu target is required and verified before forwarding.

Use the existing authenticated guest relay and vsock path. Its grant supplies
`world_id`; the hook cannot choose it. Hooks are short-timeout and fail-open.

Store the latest observation per `(world_id, session_id)`:

```text
session_id, world_id, cwd, state,
tmux_session, pane_id, received_at_unix_ms
```

Receipt time is required because hooks provide no heartbeat. World deletion
cascades to its observations.

### API

```text
session { session_id, rollout_updated_at_unix_ms?, observations[] }
observation { world_id, world_name, cwd, state, received_at_unix_ms,
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
