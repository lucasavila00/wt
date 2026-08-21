# ADR 0053: Capture Codex session activity per WT context

- Status: Proposed; Date: 2026-08-21

## Context

A WT context is one entry in `~/.wt/config.toml`, such as local or an SSH
server. Each context owns separate worlds, a shared Codex rollout directory,
and a workload registry. GitHub and GitLab gateway providers are unrelated.

Rollouts are the durable session catalog, but do not reliably say which world
and Byobu pane currently run Codex or whether that process is working or needs
attention. That live state exists inside the world, including inside its
devcontainer.

## Decision

Capture and expose context-local data only. Rendering, refreshing, resuming,
and opening sessions in `wt shell` are outside this decision.

For each context:

```text
shared rollouts ------------------------------> wt-server
Codex hook -> guest relay -> authenticated vsock -> gateway -> registry
                                                        |
                                                        +-> wt-server API
```

`wt-server` reads session IDs and rollout timestamps from its fixed shared
rollout directory. It validates only the bounded first `session_meta` record,
requires a canonical UUID, and excludes subagent rollouts. Rollout internals
are not used to infer live state.

WT installs silent, synchronous, short-timeout, fail-open hooks for
`SessionStart`, `UserPromptSubmit`, `Stop`, and `SessionEnd`. They report
`unknown`, `working`, `needs_attention`, and `inactive`, respectively. See the
[Codex hooks documentation](https://developers.openai.com/codex/hooks/).

Every report requires a complete Byobu target: the fixed tmux session and its
`%N` pane ID. Host hooks read `TMUX_PANE`; the devcontainer pane bridge validates
and injects `WT_BYOBU_SESSION` and `WT_BYOBU_PANE`. The guest relay verifies that
the pane currently belongs to that session before forwarding the report.

Reuse the existing agent-tool relay, vsock port, and per-world grant. Add no
endpoint or watcher daemon. The grant determines `world_id`; client-supplied
world identity is never accepted.

Store one latest observation per `(world_id, session_id)` in a dedicated table:

```text
session_id, world_id, cwd, state,
tmux_session, pane_id, received_at_unix_ms
```

World deletion cascades to its observations. A report is advisory: any process
in the world can imitate Codex, and an old report can outlive the process. The
API therefore exposes receipt time without discarding the stored state or pane.
Consumers decide freshness because hooks do not provide a heartbeat. Multiple
world observations for one session remain visible; the server must not silently
choose one.

The context API groups the durable catalog and all retained observations by
`session_id`. A rollout-only session has no observations. A hook may create a
report-only session briefly before its rollout becomes visible.

```text
session
  session_id
  rollout_updated_at_unix_ms?       # absent for a report-only session
  observations[]
    world_id, world_name, cwd, state, received_at_unix_ms
    target { tmux_session, pane_id } # complete for every observation
```

## Multiple contexts

There is no cross-context database or replication. A consumer queries every
configured context independently, attaches the client-side context name to
each response, and preserves failures per context. The server response cannot
contain that name because aliases such as `ars` exist only in the client config.

The routable identity is `(context, session_id)`, and a live location is
`(context, world_id, tmux_session, pane_id)`. If the same Codex UUID appears in
two contexts, a consumer may group those records for display but must retain
both locations and must not silently deduplicate or select one.

## Consequences

- Session files, credentials, and registry rows remain inside their context.
- Disabled, failed, skipped, or crash-lost hooks leave missing or stale data.
- Protocol, registry, gateway, guest, and installer changes ship together.
- `wt shell` aggregation and interaction require a separate decision.
