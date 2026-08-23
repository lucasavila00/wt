# Proposed ADR: Detect possibly stuck Codex sessions from terminal quiescence

- Status: Proposed
- Date: 2026-08-23

## Problem

Codex can wait for input without exposing the semantic signal required by
[Report Codex input requests explicitly](adr-report-codex-input-requests.md).
Treating every old `working` observation as attention would also flag healthy
long-running commands and reasoning.

## Decision

Use WT's existing parsed terminal cells as a fallback screen-diff signal. Do
not add image capture, OCR, another SSH connection, or prompt-text matching.

Evaluate the fallback only while WT has validated that a world playback stream
is focused on exactly one live Codex target. Disable it when focus is ambiguous,
fails, or changes.

For that target, start a local timer when WT first observes `working` with a
validated stream. Show `possibly stuck` on its live-session card after two
minutes with none of these events:

- a Codex lifecycle event;
- a change to the parsed character grid, excluding cursor and style changes; or
- user input forwarded to that stream.

Reset the timer on any of those events. Suspend it on disconnect, reconnect,
resize, or loss of validated focus. Keep `possibly stuck` distinct from
`needs_attention`: it does not overwrite server lifecycle state and clears as
soon as activity resumes. Do not apply it to an inactive, unknown, compacting,
or already-attention-needed session.

## Consequences

WT can surface an unreported prompt without understanding its wording. Silent
commands and long reasoning can produce false positives, so the fallback is
deliberately tentative and is not persisted or returned by the server API.
