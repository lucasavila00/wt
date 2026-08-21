# ADR 0053: Report Codex session activity to `wt shell`

- Status: Proposed; Date: 2026-08-21

## Context

The Codex activity must list durable sessions, associate live sessions with a
world, and show which sessions need attention.

Rollouts already live in the server-backed shared sessions directory. They are
the durable catalog, but cannot identify which world currently owns a session
or whether Codex is working or waiting. That state exists where Codex runs,
including inside a devcontainer.

## Decision

`wt-server` discovers session IDs and timestamps from its fixed shared rollout
directory, using the same bounded validation as `wt-codex-integration`.
WT-owned Codex lifecycle hooks add world association, working directory, and
advisory live state. Every hook report requires its concrete Byobu target: the
world's fixed tmux session and a `%N` pane ID. Host hooks read `TMUX_PANE`; the
devcontainer pane bridge validates it in the guest and injects it into the SSH
remote command as `WT_BYOBU_PANE`.

`UserPromptSubmit` means `working`; `Stop` approximately means
`needs_attention`; `SessionEnd` means `inactive`; `SessionStart` establishes
only `unknown`. A later resume may make an inactive session live again. Hook
payloads are the supported integration point; rollout contents are not parsed
to infer live state. See the [Codex hooks documentation](https://developers.openai.com/codex/hooks/).

```text
Codex hook -> existing guest Unix relay -> existing authenticated vsock
           -> agent-tool gateway -> shared registry -> wt-server -> wt shell
```

Add a typed session-event operation to the current agent-tool relay protocol.
Reuse its vsock port and per-world grant; do not add another endpoint or a Codex
watcher daemon. The reporter emits no output, runs synchronously for ordering,
has a short timeout, and fails open.

Store latest reports separately from agent-tool feedback, keyed by world and
session. The relay rejects a target that is absent, malformed, or not in the
world's active Byobu session. Codex outside Byobu remains in the durable catalog
but has no live report. Record server receipt time, expire stale live reports to
`unknown`, and discard reports when their world is deleted. The grant identifies
only the world; pane validation is not process authentication.

`wt-server` exposes the merged, owner-scoped session view through the existing
control plane. `wt shell` groups live sessions by world, puts
`needs_attention` first, and labels missing or expired reports `unknown`.
Selecting a live session makes the guest entry helper revalidate and select the
reported pane before attaching; failure changes the session to `unknown`.
Resuming an inactive session requires an explicit target world and creates a
new pane association.

## Consequences

- Session files and authentication never cross a new boundary.
- Disabled, failed, skipped, or crash-lost hooks degrade to `unknown`.
- The gateway, registry, and control protocols need coordinated typed changes.
