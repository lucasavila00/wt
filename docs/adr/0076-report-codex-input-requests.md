# ADR 0076: Let Codex tell WT when it is waiting for an answer

- Status: Accepted
- Date: 2026-08-23

## Context

WT currently learns whether Codex is working or needs attention from lifecycle
hooks. A submitted prompt marks the session as working. The `Stop` hook marks it
as needing attention when an agent turn finishes.

That works for ordinary conversation, but not for every question displayed by
the Codex CLI. Some questions come from the CLI itself rather than from the
agent. For example, Codex can pause to ask whether it should switch to a smaller
model when availability or usage limits change. That dialog does not end the
agent turn and does not produce the `Stop` hook WT relies on. WT therefore keeps
showing the session as working while Codex is waiting for the user.

The wording on the screen is not a dependable interface. It can change between
Codex releases, be translated, or be drawn as a dialog without appearing in the
conversation rollout. A question mark in an agent message is also not proof
that the session is blocked.

## Decision

Codex must expose an explicit lifecycle event whenever an in-session CLI prompt
starts or stops waiting for user input. Model-selection dialogs, approvals, and
questions asked by the agent all use this event. Each opened prompt has an
identity that its resolution event repeats, so resolving one prompt cannot hide
another prompt that is still open.

WT will install a handler for that event alongside its existing Codex hooks.
The handler will send the session and pane identity through the existing guest
relay. The server will apply the same pane generation and ordering checks used
for the other lifecycle events, so a delayed event from an old session cannot
change the current session.

The first open prompt changes the session to `needs_attention`. Resolving a
prompt removes only that request. When none remain, a resume event changes the
session back to `working`; the existing `SessionEnd` event still changes it to
`inactive`. The events describe the transitions, so WT does not need to copy the
prompt text or understand why Codex asked it.

This decision covers prompts shown after a Codex session exists. Login and
other startup prompts have no session identity and remain outside the session
attention tracker.

Until Codex exposes this event, WT cannot make this classification reliably.
WT can use the tentative fallback defined in
[ADR 0077](0077-detect-possibly-stuck-codex-sessions.md).

## Alternatives considered

- Parse agent messages or rollout records. This misses CLI-owned dialogs and
  mistakes ordinary prose for blocking questions.
- Match known prompt text in the terminal. This couples WT to presentation text
  and requires a new rule whenever Codex changes a dialog.
- Treat every completed turn as the same kind of attention. This preserves the
  current bug because the smaller-model dialog can appear before a turn ends.

## Consequences

WT will have a reliable answer to “is this session waiting for me?” once the
Codex event is available. New kinds of in-session prompts will work without WT
learning their wording.

The semantic fix depends on a change to Codex's integration surface; WT cannot
implement it entirely on its own. Worlds running an older Codex release can
still miss the exact attention state and must rely on the less certain fallback.
Keeping prompt contents out of the event also means WT can say that input is
needed without explaining the question on the session card.
