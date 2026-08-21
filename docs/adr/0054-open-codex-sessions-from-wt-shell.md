# ADR 0054: Open Codex sessions from `wt shell`

- Status: Proposed; Date: 2026-08-21

## Problem

The Codex activity is a dense, launch-time table. It shows session reports but
cannot select or open the reported Byobu pane.

## Decision

### Cards

Render one vertically scrollable column of fixed-height cards. One card is one
location:

```text
┌ 󰚩 NEEDS ATTENTION · 18s ago ──────────────────────────┐
│ ars.wt · wt-app:%3 · session 123e4567                 │
│ /workspace/wt                                         │
└ Enter or click to open ───────────────────────────────┘
```

Card identity is `(context, session_id, world_id, tmux_session, pane_id)`.
Never merge multiple locations for the same session into one clickable card.
Rollout-only sessions and context failures use disabled cards.

Sort by `needs_attention`, `working`, `unknown`, `inactive`, then newest report.
State is always glyph plus text; color is supplementary.

### Interaction

- `Up` and `Down` move selection; the viewport follows it.
- Mouse wheel scrolls the cards and left click opens the hit card.
- `Enter` opens the selected card. Moving selection never opens it.
- `Tab`, `F1`, `F5`, and global `F6` keep their current meanings.
- There is no hover state or multi-column layout.

```text
Browse --Enter/click--> Opening --success--> active world view
  ^                         |
  └──── visible error ──────┘
```

### Validation and diagnostics

A card is openable only when all of these hold:

- the complete context response decoded without unknown or malformed fields;
- `(context, world_id, world_name)` exactly matches the shell inventory;
- the world has an existing playback PTY and direct `-vs` SSH alias;
- `tmux_session` matches the world kind and `pane_id` is `%` plus digits.

Reject the whole context snapshot when its response is invalid. Do not salvage
records, match by display name, choose another observation, or guess a target.
Show a persistent error card containing the context and failed invariant.

Opening revalidates that the pane still belongs to the reported tmux session.
Failure returns to the same selected card and shows context, world, session,
target, and the exact failed check. It is never silent.

### Opening

The card emits a typed open intent; rendering does not construct commands.
The shell session layer runs the validated focus operation through the world's
direct `context.world-vs` SSH alias, then switches to its existing playback PTY.
The operation selects the pane's window and pane only after the exact tmux
session/pane check succeeds.

The short control connection never replaces or restarts any playback SSH/PTTY.
All world sessions continue running and parsing output in the background.

Inactive or rollout-only sessions are not resumable. Refresh and resume are
separate decisions; the initial implementation keeps the current launch-time
snapshot and always detects stale targets during open.

## Source precedent

VS Code separates list selection from activation and routes open operations out
of the renderer. Its session rows foreground state, title, location, and age.

- [List open behavior](https://github.com/microsoft/vscode/blob/main/src/vs/platform/list/browser/listService.ts#L676-L766)
- [Session row hierarchy](https://github.com/microsoft/vscode/blob/main/src/vs/sessions/contrib/sessions/browser/views/sessionsList.ts#L600-L756)

## Consequences

- Duplicate reports remain visible and independently routable.
- Terminal resize cannot reorder selection.
- Opening a pane changes the active window for other clients attached to that
  same tmux session.
