# Proposed ADR: Report Codex input requests explicitly

- Status: Proposed
- Date: 2026-08-23

## Problem

The `Stop` hook does not cover every prompt shown by Codex. In particular,
Codex can ask an infrastructure question, such as whether to use an available
smaller model, while WT continues to report the session as working.

## Decision

Require a Codex lifecycle hook emitted whenever an in-session interactive CLI
blocks for user input, including model availability, approval, and agent
questions. The hook must identify the session and whether the request opened or
resolved. Report an opened request as `needs_attention`. Clear it only when a
later pane-ordered hook resolves that request or moves the session to `working`
or `inactive`.

Do not parse rollout text, assistant text, or punctuation to synthesize the
semantic event. Until the installed Codex version provides the hook, leave
semantic state unchanged and use the fallback defined in
[Detect possibly stuck Codex sessions](adr-detect-possibly-stuck-codex-sessions.md).

## Consequences

WT can report all in-session blocking prompts through the existing authenticated,
pane-ordered lifecycle path once Codex exposes the hook. The fallback remains
necessary for older Codex versions and any prompt surface that omits it.
