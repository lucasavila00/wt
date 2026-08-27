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

An observer fingerprint is not itself a preview: it only detects whether a
captured pane changed. The observer already captures each eligible pane to
produce that fingerprint, so it can provide a bounded rendered pane frame
alongside it.

## Decision

The Live activity renders each card from the latest rendered frame captured
for that card's observed Codex pane. It does not use any SSH playback parser
as a Live preview source.

The guest observer captures every Byobu pane whose foreground process is
`codex`. For every capture it reports the observation identity, a normalized
and bounded terminal frame, and its fingerprint. The fingerprint remains the
change-detection and freshness key; the frame is the visual data. “Screenshot”
here means that normalized terminal frame, not a raster image and not terminal
bytes from a client SSH session.

`wts` owns the latest frame for each `(world, tmux session, pane)` observation
and serves it only to the world's authorized clients. It keeps no history of
frames and discards a frame with its observation. A server restart may leave a
preview unavailable until the next guest observation; it must never substitute
another pane's frame. Frames remain bounded and are normalized before they
cross the authenticated guest-to-server boundary.

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
- The observation path carries the latest visible terminal content, so its
  bounded payload and owner-scoped access are security-relevant.
