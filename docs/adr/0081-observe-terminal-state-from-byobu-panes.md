# ADR 0081: Observe terminal state from Byobu panes

- Status: Proposed
- Date: 2026-08-26

## Context

WT currently builds its live Codex activity model from Codex lifecycle hooks.
The hooks identify a Codex session, report state transitions, and cause WT to
persist per-session state, pane generations and event sequences. The guest
relay then supplements those reports with pane liveness and checkout polling.
That design duplicates information that is already visible in the terminal and
is coupled to Codex hook behavior and private session concepts.

Reading the client playback screen alone is not a replacement. A client sees
only the pane currently selected in its shared Byobu session, can be changed by
another tmux client, and has no view of the other panes in a world. Its result
would be local to one running `wt shell`, rather than shared state.

## Decision

Make rendered Byobu panes the source of live terminal state. A WT-owned guest
observer enumerates the world's panes and reads their rendered screens. It
reports bounded, normalized pane observations through the existing
authenticated guest-to-server path. `wts` owns the resulting state and serves
it to every client.

An observation is identified by world, tmux session, and pane. It contains
only generic screen-derived facts needed by the UI, such as the observation
time, whether the rendered screen changed, and a bounded display summary or
classification. It is not a Codex session record: it has no Codex session ID,
hook event, lifecycle state, compaction phase, working directory, checkout
state, pane generation, or event sequence. Do not persist raw terminal
contents, credentials, or complete screen history.

The observer reads every eligible Byobu pane in the guest, not the client's
currently visible pane. It uses a defined bounded polling interval and reports
only meaningful changes or freshness updates. Loss of observation is displayed
as stale or unavailable; it must not be converted into an inferred application
exit or a synthetic lifecycle transition.

`wt shell` renders server-provided pane observations as world terminal
activity. Opening a world continues to attach to its terminal normally. It does
not claim that a screen observation identifies a particular Codex conversation,
and it does not switch panes merely to create or validate activity state.

Remove the Codex lifecycle reporting path: the configured `report-hook`
commands, hook payload parsing, relay registration and pane-marker tracking,
lifecycle event protocol, `codex_session_reports` persistence, and the
per-session UI state built from them. Remove dependent Codex-specific liveness,
compaction-expiry, pane-supersession, and checkout-observation logic.

Retain Codex integration only where a rendered screen cannot replace it:
shared authentication, startup-time history synchronization, and access to
durable rollout history. Those facilities must not create or update live pane
state. Any history UI remains explicitly historical rather than a claim about
what is currently running.

## Migration

1. Define the bounded pane-observation protocol, registry model, retention,
   freshness behavior, and screen normalization with tests using representative
   terminal screens.
2. Add the guest observer and server query path, then render world-level pane
   activity from that state in `wt shell`.
3. Remove the Codex hook configuration, reporting protocol, relay tracking,
   lifecycle registry state, and session-specific live UI. Delete or rewrite
   the ADRs that describe those replaced decisions.
4. Treat the registry change as incompatible and use the existing `make clear`
   deployment reset rather than migrating obsolete Codex lifecycle records.

## Consequences

- Live state is shared through `wts` and covers every observed Byobu pane, not
  just one client's current playback pane.
- WT depends on stable rendered-terminal semantics rather than Codex hook
  ordering and lifecycle coverage. Screen classifications are observations and
  can be unknown or stale; the UI must communicate that uncertainty.
- Codex upgrades no longer require WT to track their hook payloads or repair
  stranded lifecycle state.
- The observer introduces guest polling and server-side pane state, but removes
  Codex-specific state machines, sequencing, reconciliation, and checkout
  tracking.
- The implementation supersedes the live-state portions of ADRs 0029, 0032,
  0033, 0040, 0045, 0074, 0075, both ADR 0076 decisions, and ADR 0080. The
  implementation must fold any still-applicable behavior into this decision
  and remove records that no longer describe WT.
