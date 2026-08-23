# ADR 0040: Supersede Codex sessions by pane

- Status: Accepted; Date: 2026-08-22

## Problem

Codex `/reset` starts a new session in the existing Byobu pane but does not
reliably report `SessionEnd` for the previous session. WT therefore retained
the previous session's `working` or `needs_attention` state after the new
session had started.

## Decision

A non-inactive Codex lifecycle observation makes its session the current
session for that `(world_id, tmux_session, pane_id)`. In the same database
transaction, WT marks every other non-inactive session for that pane inactive
before storing the new observation.

An inactive observation updates only its own session. This prevents a delayed
`SessionEnd` for a superseded session from deactivating its replacement.

This is an inferred lifecycle transition at the time WT receives the
replacement observation. Rollout files remain the durable session catalog, so
superseded sessions continue to appear as inactive saved sessions.
