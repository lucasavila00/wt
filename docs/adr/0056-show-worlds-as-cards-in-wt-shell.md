# ADR 0056: Show worlds as cards in `wt shell`

- Status: Proposed; Date: 2026-08-21

## Decision

The Worlds activity renders one vertically scrollable column of cards. It uses
the same card component and interaction rules as the Codex activity: border,
state glyph and text, information hierarchy, selected/opening/error states,
viewport behavior, keyboard navigation, wheel scrolling, and exact click
hit-testing.

Each inventory world has one card identified by `(context, world_id)`. A card
shows every field currently emitted by `wt ls`:

- context and name;
- kind and status;
- repository;
- CPU, memory, and disk allocation/usage;
- detail, including last error, report count, and recovery commands.

Cards may wrap values but never replace missing, invalid, or failed data with a
guess. Context fetch failures are visible and no successfully decoded world is
silently omitted. The panel retains the independent refresh timestamp defined
by ADR 0055.

`Up` and `Down` move selection. The mouse wheel moves selection and its
viewport. `Enter` or a left click opens the selected running world in its
existing full-screen playback view. Selection alone has no side effect.

A world is openable only when its typed inventory identity maps to a live
playback PTY. Provisioning, stopped, failed, or otherwise unavailable cards are
disabled and show the exact reason. Opening never creates or reconnects an SSH
session. Failure leaves the Worlds activity, selection, and error visible.

World and Codex cards share rendering and interaction primitives, not copied
implementations. Activity-specific code supplies identity, state, fields,
disabled reason, and open action. `Tab`, `F1`, `F5`, and `F6` keep their current
meanings.

## Consequences

- The Worlds activity replaces the placeholder; `wt ls` remains available.
- Resize cannot change identity or selection.
- World-management actions other than opening remain separate work.
