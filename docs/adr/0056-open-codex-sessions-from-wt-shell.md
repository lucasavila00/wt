# ADR 0056: Open Codex sessions from `wt shell`

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

Observation-card identity is
`(context, session_id, world_id, tmux_session, pane_id)`. Never merge multiple
locations for the same session into one clickable card. `inactive`,
rollout-only, and context-error cards are disabled.
Rollout-card identity is `(context, session_id)`; context-error identity is
`context`. Disabled cards state why they cannot open.

Sort by `needs_attention`, `working`, `unknown`, `inactive`, `rollout-only`,
then `context-error`; next by descending report/rollout timestamp, then the
complete identity. A missing timestamp sorts as zero. State is always glyph
plus text; color is supplementary.

### Interaction

- `Up` and `Down` move selection; the viewport follows it.
- Mouse wheel moves selection and the viewport together. Left click opens the
  card whose rendered rectangle was hit.
- `Enter` opens the selected card. Moving selection never opens it.
- A disabled-card click or `Enter` is consumed and its reason remains visible;
  it performs no network operation.
- While opening, selection and the displayed card snapshot stay pinned until
  success, failure, or timeout.
- `Tab`, `F1`, `F5`, and global `F6` keep their current meanings.
- There is no hover state or multi-column layout.

```text
Browse --Enter/click--> Opening --success--> active world view
  ^                         |
  └──── visible error ──────┘
```

### Validation and diagnostics

A context produces cards only when all of these hold:

- an operation-specific response envelope decoded without unknown or malformed
  fields at any level; generic response decoding is insufficient;
- every identity is unique, every timestamp is nonnegative, and every `cwd` is
  absolute, at most 4096 bytes, and contains no control characters;
- `(context, world_id, world_name)` exactly matches the shell inventory;
- the world has an existing playback PTY and its required control SSH alias;
- `tmux_session` matches the world kind and `pane_id` is `%` plus 1–16 ASCII
  digits.

Reject the whole context snapshot when its response is invalid. Do not salvage
records, match by display name, choose another observation, or guess a target.
Show a persistent error card containing the context, failed invariant, and a
bounded escaped offending value.

Store selection as the complete card identity; hit-testing never uses a label or
truncated UUID. Opening revalidates the playback PTY and remote target. Failure
keeps the same selection and active world, and shows context, world, session,
target, and the exact failed check. It is never silent.

### Opening

The card emits a typed open intent; rendering does not construct commands.
Retain launch inventory and playback indices keyed by `(context, world_id)`;
never reconstruct them from rendered names.

On accepted non-inactive reports, the guest relay writes the pane-local tmux
option `@wt_codex_session_id`. `SessionEnd` removes it only when it still equals
that session UUID.

The shell session layer runs one WT-owned focus helper through
`context.world-host` for a dev world or `context.world-vs` for a host world, as
selected from typed inventory. The helper requires session, pane, pane marker,
and `pane_dead` to equal
`<tmux_session>:<pane_id>:<session_id>:0`. It then derives and selects the pane's
window before selecting the pane, and returns exactly that value followed by one
LF. Any other bytes, mismatch, or nonzero status is a visible failure.

After focus succeeds, switch to the mapped existing playback PTY and world view.
The active world never changes before success.

The short control connection has a 15-second whole-operation deadline. It never
replaces or restarts any playback SSH/PTTY. All world sessions continue running
and parsing output in the background.

Inactive and rollout-only sessions are not openable or resumable. ADR 0055
refreshes the cards in the background; each open still revalidates its target.

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
