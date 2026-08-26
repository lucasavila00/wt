# ADR 0081: Observe terminal state from Byobu panes

- Status: Proposed
- Date: 2026-08-26

## Context

WT currently builds its live Codex activity model from Codex lifecycle hooks.
The hooks identify a Codex session, report state transitions, and cause WT to
persist per-session state, pane generations, and event sequences. The guest
relay supplements those reports with pane liveness and checkout polling. This
duplicates information already visible in the terminal and couples the live UI
to Codex hook behavior and private session concepts.

Reading a client playback screen alone is not a replacement. A client sees only
the pane currently selected in its shared Byobu session, another tmux client can
change that selection, and the result exists only while that client runs.

## Decision

Make rendered Byobu panes the source of live terminal state. A WT-owned guest
observer enumerates every eligible pane and reads its rendered terminal screen.
It reports bounded, normalized pane observations through the existing
authenticated guest-to-server path. `wts` owns the resulting state and serves
it to every client.

An observation is identified by world, tmux session, and pane. It contains only
generic screen-derived facts needed by the UI, such as the observation time,
whether the rendered screen changed, and a bounded display summary or
classification. It has no Codex session ID, hook event, lifecycle state,
compaction phase, working directory, checkout state, pane generation, or event
sequence. Do not persist raw terminal contents, credentials, or complete screen
history.

The observer uses a defined bounded polling interval and sends only meaningful
changes or freshness updates. A missing or failed observation is stale or
unavailable state; it must not be converted into an inferred application exit
or a synthetic lifecycle transition.

`wt shell` renders server-provided pane observations as world terminal
activity. Opening a world continues to attach to its terminal normally. It does
not claim that an observation identifies a particular Codex conversation, and
it does not switch panes merely to create or validate activity state.

Deliver this as one incompatible cutover. In the same release, remove the
Codex lifecycle integration: configured `report-hook` commands, hook payload
parsing, relay registration and pane-marker tracking, lifecycle event protocol,
`codex_session_reports` persistence, and the per-session UI state built from
them. Remove dependent Codex-specific liveness, compaction-expiry,
pane-supersession, and checkout-observation logic. Do not run both live-state
systems during a transition.

Retain Codex integration only where a rendered screen cannot replace it:
shared authentication, startup-time history synchronization, and access to
durable rollout history. Those facilities must not create or update live pane
state. Any history UI remains explicitly historical rather than a claim about
what is currently running.

## Migration

Implement and deploy the pane-observation protocol, registry model, guest
observer, server query, and world-level shell UI together with removal of the
Codex lifecycle pipeline. Test the bounded normalization, freshness, and UI
behavior with representative terminal screens. Treat the registry change as
incompatible and use the existing `make clear` deployment reset rather than
migrating obsolete Codex lifecycle records.

Delete or rewrite the ADRs that describe replaced live-state decisions as part
of the same change.

## Consequences

- Live state is shared through `wts` and covers every observed Byobu pane, not
  just one client's current playback pane.
- WT depends on stable rendered-terminal semantics rather than Codex hook
  ordering and lifecycle coverage. Screen classifications are observations and
  can be unknown or stale; the UI must communicate that uncertainty.
- Codex upgrades no longer require WT to track hook payloads or repair
  stranded lifecycle state.
- The observer introduces guest polling and server-side pane state, while the
  cutover removes Codex-specific state machines, sequencing, reconciliation,
  and checkout tracking.
- The implementation supersedes the live-state portions of ADRs 0029, 0032,
  0033, 0040, 0045, 0074, 0075, both ADR 0076 decisions, and ADR 0080. The
  implementation must fold any still-applicable behavior into this decision
  and remove records that no longer describe WT.
