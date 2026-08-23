# ADR 0076: Detect pending Codex input requests from rollouts

- Status: Accepted
- Date: 2026-08-23

## Decision

Extend WT's rollout catalog to track Codex input-request tool calls and their
responses.

If a session has an input request without a response, report it as
`needs_attention`. Clear that state when the rollout records the response.

Do not change Codex. CLI-owned dialogs that are not written to the rollout use
the fallback in ADR 0077.

## Consequences

WT detects agent questions without depending on the `Stop` hook. It cannot
identify unrecorded CLI dialogs, including the smaller-model prompt.
