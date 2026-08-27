# ADR 0082: Render live previews from observed Codex pane captures

- Status: Proposed
- Date: 2026-08-27

## Context

The Live activity currently identifies each Codex Byobu pane from a server
observation, but renders its card from the world-level SSH playback parser.
That parser follows the Byobu pane currently active in the shared window. It
therefore can render a non-Codex tab, or one Codex pane's screen in every
Codex card for that world. A paused card is especially misleading because its
state is derived from one pane while its pixels can come from another.

An observer fingerprint is not itself a preview: it is an irreversible digest
used only to detect whether a captured pane changed. The observer already
targets and captures each eligible pane to produce that fingerprint, so that
same pane-local observation path can also provide the visual data.

## Decision

This replaces the Live preview-source decision in ADR 0081 and is folded into
that record when implemented.

The Live activity renders each card from the latest rendered frame captured
for that card's observed Codex pane. It does not use any SSH playback parser
as a Live preview source.

The guest observer captures every Byobu pane whose foreground process is
`codex`. For each pane-local observation it reports the observation identity,
the existing normalized fingerprint, and a viewport-only terminal frame. The
fingerprint determines `changed_at`; successful receipt of the report
determines `observed_at`; the frame is only the visual data. Every transmitted
observation includes its frame, and an accepted report replaces that frame
even when the fingerprint did not change.

“Screenshot” here means an inert rectangular grid of displayed cells, including
its dimensions and presentation attributes. It is not a raster image, tmux
scrollback, or an ANSI/control byte stream that a client replays. The observer
consumes any tmux capture escapes before transport. The client preserves the
captured row and column coordinates, clipping or padding the grid to fit a
card rather than reflowing it. This makes the preview a faithful view of the
observed pane while preventing pane content from becoming local terminal
control input.

`wts` holds the latest frame in memory for each `(world, tmux session, pane)`
observation and serves it only to the world's authorized clients. Frames are
not persisted or logged. It replaces the world's frame set as one observation
snapshot, so removal of a pane also removes its frame. A frame is published
only with the exact observation identity and `observed_at` value produced by
the same accepted report; this prevents a delayed frame or a reused tmux pane
ID from being paired with newer metadata. A server restart leaves previews
unavailable until the next guest report; it must never substitute another
pane's frame.

Both sides apply explicit bounds, and `wts` independently validates the pane
count, frame dimensions, per-frame encoded size, total report size, cell text,
and style values. A malformed or oversized report is rejected as a whole
without refreshing its observations. The protocol admits only inert cell data,
not terminal control sequences. These checks apply before a frame is retained
or returned by the owner-scoped control API.

Each observed Codex pane has one independent Live card. Multiple Codex panes
in one world therefore show their own captured frames, even when another
Byobu client changes the shared window's active pane. Stale cards may show
their last captured frame only when their stale status is explicit; missing
frames are unavailable rather than guessed.

Opening a card still targets its exact observed pane through the existing SSH
control path, then transitions to the full world Byobu view. SSH playback is
used only for that full world view, where showing the selected shared Byobu
window is intentional.

## Consequences

- A Live preview cannot display a non-Codex tab or a different Codex pane.
- Multiple Codex panes in a world are correctly represented at the same time.
- Live previews remain shared server observations rather than client-local SSH
  playback state.
- The observation path carries the latest visible terminal content, which may
  contain secrets. It is therefore ephemeral, bounded, validated on receipt,
  excluded from logs, and exposed only through owner-scoped access.
