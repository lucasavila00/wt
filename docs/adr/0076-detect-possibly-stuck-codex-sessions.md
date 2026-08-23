# ADR 0076: Flag quiet Codex sessions as possibly stuck

- Status: Accepted
- Date: 2026-08-23

## Decision

When WT has verified that a live terminal stream shows one Codex session, mark
the session `possibly stuck` after 30 seconds with no visible character changes
and no newer Codex lifecycle observation or user input.

Allow the threshold to be changed with `shell.codex_stuck_after_seconds` in the
client configuration.

Use WT's parsed terminal screen. Do not capture screenshots, run OCR, or match
prompt text. Keep `possibly stuck` as a local UI hint; do not store it as session
state.

## Consequences

WT can implement this without changing Codex, but silent work can produce false
positives. Do not apply the fallback when WT cannot identify the displayed pane.
