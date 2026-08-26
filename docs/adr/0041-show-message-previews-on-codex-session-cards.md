# ADR 0041: Show message previews on Codex session cards

- Status: Accepted; Date: 2026-08-22

## Decision

- Extend each Codex session inventory item with an optional latest-user-message
  preview and its event timestamp.
- Derive the preview server-side from the rollout's latest user-message event;
  do not send the complete message or rollout to the client.
- Normalize whitespace, remove terminal control sequences, and bound the preview
  by UTF-8 bytes before returning it.
- Keep sessions whose rollout is missing, unreadable, or has no user message;
  return no preview for them.
- Render each session location as a taller card with, in order:
  - state and relative activity time;
  - the wrapped latest-user-message preview;
  - context, world, repository or working directory, and Git branch;
  - tmux session, pane, and abbreviated Codex session ID;
  - the existing open action or disabled reason.
- Allocate up to three lines to the preview and truncate overflow with an
  ellipsis. Omit the preview rows when no preview exists.
- Preserve the sorting, identity, selection, scrolling, click targets, refresh
  behavior, and open action defined by ADRs 0055 and 0056.
- Treat preview text as untrusted display data and never interpret ANSI escapes,
  terminal controls, or markup.

## Consequences

- More sessions fit below the fold only through scrolling.
- Message previews may expose prompt text to anyone who can view `wt shell`.
- The durable rollout catalog reads the latest user-message event in addition to
  `session_meta`.
