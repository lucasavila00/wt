# Proposed ADR: Flag a quiet Codex session as possibly stuck

- Status: Proposed
- Date: 2026-08-23

## Context

WT needs a fallback for Codex versions that cannot explicitly report that they
are waiting for input. The original suggestion called this an OCR-like approach:
if a session still claims to be working but its screen has not changed for two
minutes, bring it to the user's attention.

WT does not need screenshots or OCR to do that. `wt shell` already maintains a
parsed terminal screen for each connected world so it can draw live previews.
That screen is a grid of characters and styles. Comparing the characters gives
WT a cheap way to notice meaningful visible changes without interpreting the
words on the screen.

There is an important limitation. WT has one playback stream per world, not one
per Codex session. It can use the stream as evidence for a particular session
only when it has successfully focused that stream on the session's pane. A world
with multiple possible targets, or a failed pane focus, provides no trustworthy
per-session screen signal.

## Decision

When a playback stream is known to be focused on exactly one live Codex session,
WT will watch the characters in its parsed terminal screen. If Codex still
reports `working` and the screen remains unchanged for two minutes, the live
session card will show `possibly stuck` in the attention color.

The two-minute timer starts when WT first has both a working session and a
validated focused stream. It restarts when:

- the visible characters change;
- Codex reports another lifecycle event; or
- the user sends input to that stream.

Cursor movement, cursor blinking, and color or style changes do not count as
progress. They can change while a dialog is waiting and would otherwise keep
the session looking active forever.

WT suspends the timer when the stream disconnects, reconnects, resizes, changes
focus, or can no longer be tied to one Codex session. It starts a fresh timer
after the association becomes trustworthy again; it does not guess how long the
screen was quiet while WT could not observe it.

`possibly stuck` is deliberately different from `needs_attention`. It is a
local hint on the live card, not a new server-side lifecycle state. It is not
stored, returned by the API, or shown for sessions already known to need
attention. Observed terminal activity removes it on the next redraw; a Codex
lifecycle event removes it on the next inventory refresh.

## Alternatives considered

- Capture screenshots and run OCR. WT already has the terminal characters, so
  images would add cost and ambiguity while recovering less accurate text.
- Match known dialogs or look for question marks. This is brittle across Codex
  releases and confuses visible text with interaction state.
- Use only the age of the last lifecycle hook. Silent commands and long periods
  of reasoning can be healthy even when no hook arrives.
- Apply the heuristic to every session in a world. One world stream cannot show
  several panes at once, so this would attribute one pane's activity to another.

## Consequences

The fallback can draw attention to the smaller-model dialog and other quiet
prompts even when Codex provides no semantic event. It reuses information WT
already has and does not inspect or classify the displayed text.

The signal is only a suspicion. A silent build, a long-running command, or a
long reasoning step can be flagged even though nothing is wrong. A prompt with
an animation that changes visible characters can avoid detection. Sessions in
ambiguous or unfocused panes receive no fallback at all. The wording `possibly
stuck` makes these limits visible instead of presenting a heuristic as fact.
