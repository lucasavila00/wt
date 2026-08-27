# ADR 0075: Open world actions from a context menu

- Status: Superseded in part by ADR 0081
- Date: 2026-08-23

## Decision

Add an actions button to the top-right edge of each world card's frame:

```text
┌ 󰚩 STATIC · NO RECENT PANE CHANGE ─────────────────────────── … Menu ┐
│ars.calm-wombat                                                     │
│2 CPU · 4G · 7.5G/32G disk                                         │
│                                                                   │
└───────────────────────────────────────────────────────────────────┘
```

Clicking the full `… Menu` label opens a contextual menu with the F1 command
palette's interaction and visual style. Its only initial option is Delete,
which opens the existing delete confirmation for that world. The menu does not
delete directly, and its button has a separate hit target from the card's open
action.

Use the complete menu now, despite having one option, so Delete is not a
prominent card action and future actions such as Rename gain a stable home
without another card redesign. Do not show placeholders for future actions.

Carry the world's stable identity through the menu and confirmation. Tests
cover keyboard and mouse use, dismissal, the correct world, cancellation, and
the button not opening the underlying card.
