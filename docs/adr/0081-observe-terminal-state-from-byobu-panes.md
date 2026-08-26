# ADR 0081: Observe terminal state from Byobu panes

- Status: Proposed
- Date: 2026-08-26

## Context

Rendered terminal state is the common evidence available for every program in a
world. It can replace the Codex-specific live-state pipeline only if it is
shared and complete. A client playback connection is neither: it sees one
currently selected pane, another tmux client can change that selection, and its
result exists only while that client runs.

WT needs server-owned state that covers every rendered Byobu pane before the
Codex lifecycle reports can be removed.

## Decision

Add a WT-owned guest observer that enumerates every eligible Byobu pane and
reads its rendered screen. It reports bounded, normalized pane observations to
`wts` through the existing authenticated guest-to-server path. `wts` persists
and serves that state to every client.

An observation is identified by world, tmux session, and pane. It contains only
generic screen-derived facts needed by the UI, such as the observation time,
whether the rendered screen changed, and a bounded display summary or
classification. It has no application session identity or application-specific
lifecycle fields. Do not persist raw terminal contents, credentials, or
complete screen history.

The observer uses a defined bounded polling interval and sends only meaningful
changes or freshness updates. A missing or failed observation is stale or
unavailable state; it must not be converted into an inferred application exit.

This decision adds a generic server-side pane-observation model. It does not
remove Codex lifecycle reports or change the existing Codex activity UI; that
work is deferred to ADR 0082.

## Migration

1. Define the bounded pane-observation protocol, registry model, retention,
   freshness behavior, and screen normalization with tests using representative
   terminal screens.
2. Implement the guest observer and server query path.
3. Validate that `wt shell` can render the resulting world and pane state while
   preserving the current Codex lifecycle UI.

## Consequences

- Terminal activity is shared through `wts` and covers every observed Byobu
  pane, not only one client's current playback pane.
- The observer introduces guest polling and server-side pane state. The state
  is bounded and generic, so later UI work is not coupled to Codex hooks.
- Screen classifications are observations and can be unknown or stale; clients
  must communicate that uncertainty.
- The new state can coexist with Codex lifecycle records until ADR 0082 removes
  the replaced tracking path.
