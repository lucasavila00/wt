# ADR 0082: Replace Codex live state with terminal state

- Status: Proposed
- Date: 2026-08-26
- Requires: [ADR 0081](0081-observe-terminal-state-from-byobu-panes.md)

## Context

Codex lifecycle hooks currently identify a Codex session, report state
transitions, and cause WT to persist per-session state, pane generations, and
event sequences. The guest relay supplements those reports with pane liveness
and checkout polling. This duplicates information already visible in the
terminal and couples the live UI to Codex hook behavior and private session
concepts.

ADR 0081 introduces a shared, server-owned source of generic state for every
rendered Byobu pane. It is the prerequisite for removing the lifecycle pipeline
without making activity local to one `wt shell` client.

## Decision

After ADR 0081 is implemented, `wt shell` renders server-provided pane
observations as world terminal activity. Opening a world continues to attach to
its terminal normally. The UI does not claim that an observation identifies a
particular Codex conversation, and it does not switch panes merely to create or
validate activity state.

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

1. Merge and deploy ADR 0081's server-side pane-observation migration.
2. Render world-level terminal activity from the resulting server query in
   `wt shell`.
3. Remove the Codex hook configuration, reporting protocol, relay tracking,
   lifecycle registry state, and session-specific live UI. Delete or rewrite
   the ADRs that describe those replaced decisions.
4. Treat the registry change as incompatible and use the existing `make clear`
   deployment reset rather than migrating obsolete Codex lifecycle records.

## Consequences

- The live UI uses shared pane observations rather than one client's current
  playback pane or Codex lifecycle reports.
- Codex upgrades no longer require WT to track hook payloads or repair
  stranded lifecycle state.
- The UI gives up Codex session identity and hook-derived labels. It must
  present terminal observations as unknown or stale when they are incomplete.
- The implementation supersedes the live-state portions of ADRs 0029, 0032,
  0033, 0040, 0045, 0074, 0075, both ADR 0076 decisions, and ADR 0080. The
  implementation must fold any still-applicable behavior into this decision
  and remove records that no longer describe WT.
