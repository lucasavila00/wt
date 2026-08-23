# WT client guidance

Before changing `wt shell` terminal behavior, trace `SessionSet` in
`src/shell/session.rs` and the main loop in `src/shell.rs`.

- `wt shell` owns one SSH/PTY playback connection, reader thread, and `vt100`
  parser for every currently SSH-openable world, not only the visible world.
- Reader threads feed a shared queue; the main loop drains it and advances every
  parser in every UI mode. Reconciliation adds and removes sessions; reconnect
  replaces one session's connection, parser, and stream identity.
- UI modes change terminal dimensions and presentation; they do not create the
  underlying playback streams.
- Codex observations already carry the world, tmux session, and pane identity
  used when selecting a pane. tmux selection is shared with other clients and
  is not durable ownership of that pane.

Do not claim that a shell UI change requires additional polling, threads,
connections, or parsing without first identifying the existing owner and
lifecycle in these code paths. When reviewing an ADR, verify its architectural
premise against the owning code, not only against the proposed implementation.
For a shared resource, identify who can change its logical owner, how long the
mapping is valid, and how invalidation is detected.
