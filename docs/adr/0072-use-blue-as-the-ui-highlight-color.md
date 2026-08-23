# ADR 0072: Use blue as the UI highlight color

- Status: Accepted
- Date: 2026-08-23

## Problem

`wt shell` used yellow both for selected card borders and for states that need
attention. Reusing one color for navigation and status makes the UI's meaning
ambiguous and weakens yellow as an attention signal.

## Decision

Use terminal palette colors consistently by meaning:

- Blue highlights UI navigation with accents such as active markers and
  selected-card borders.
- Yellow identifies states that need attention.
- Red identifies errors.
- Green identifies successful or healthy states.

Selected card borders and the active activity-rail icon and marker are blue.
Text, list, and form selections may continue to reverse terminal defaults when
that provides theme-safe contrast. Status labels retain text or symbols that
carry their meaning without color; color reinforces that meaning and is not the
only signal.

Structural surfaces and ordinary text continue to use terminal defaults as
required by [ADR 0038](0038-make-wt-shell-terminal-theme-safe.md).

## Consequences

Navigation highlights are visually distinct from states that need attention.
Future terminal UI changes use the same semantic assignments instead of
choosing colors independently.

## Verification

Test the shared selected-card border style and active activity-rail styles
directly. Snapshot tests continue to cover the complete rendered Worlds, Codex,
and Live card views.
