# ADR 0086: Manage work as Byobu windows

- Status: Accepted
- Date: 2026-09-02

## Decision

WT exposes one window resource for work inside a world. Each WT window is one Byobu tab, implemented
as one tmux window with one pane. `start_window` starts the requested executable in that pane. The
window ID addresses the executable, its input and output, and its screen.
It is a globally unique UUID. The registry persists its mapping to the guest-native tmux
`#{window_id}` (`@N`); authenticated guest tooling may resolve `(world_id, tmux_window_id)` back to
the stable window ID without exposing the native ID in the public API.

A window provides:

- lifecycle state and the executable's exit status;
- lossless, ordered standard input, standard output, and standard error; and
- the current Byobu screen as text, with its observation time.

The stable `wt api` operations are:

```text
start_window(world_id, argv, cwd) -> window, control_token
get_window(window_id, after, limit, include_screen)
  -> state, exit_code?, exit_signal?, output, cursors, screen?
send_window_input(window_id, control_token, data_base64)
stop_window(window_id, control_token)
delete_window(window_id, control_token)
```

Output is a sequence of records with a monotonic record ID, a `stdout` or `stderr` channel, and
exact bytes encoded as base64. `after` is an exclusive record ID. A response includes `next_after`
and `oldest_available`. An output gap tells the client to recover through its application protocol.

Input contains exact bytes encoded as base64. WT serializes writes in committed request order and
adds no terminal echo to the output records. WT acknowledges input after it commits the bytes to the
window input queue. A dedicated guest input socket feeds the executable's standard input without
terminal line-discipline transformation; tmux is only the screen view. The guest drains that queue
in order and deduplicates an already acknowledged sequence. The `wt api` request ID gives start,
input, stop, and delete a stable durable identity during the 30-day idempotency period. Start
reserves a deterministic window ID before launch and recovers a matching tmux window after an
interrupted reply. Each input request has at most one durable queue record and sequence ID.

A machine failure in the irreducible interval between an arbitrary process consuming bytes and the
guest persisting its sequence acknowledgement can still cause at-least-once delivery. Applications
that require recovery across guest failure must deduplicate messages in their own protocol.

The screen is a bounded plain-text rendering of the same Byobu window. Machine output and the screen
are two views of one window. `include_screen` lets polling clients request the screen only when they
need it.

## Ownership

WT owns the Byobu window, executable launch, input and output, status, screen rendering, and
cleanup. A client chooses the executable, arguments, and working directory.

The client that starts a window holds its opaque control token. Screen and output reads use owner
authorization. Input, stop, and delete also require the control token. `wt shell` presents a
controlled window as read-only. This gives the machine protocol one writer.

Apr starts Codex App Server in a WT window. It reads JSON-RPC from `stdout`, reads diagnostics from
`stderr`, writes JSON-RPC to standard input, and uses the Byobu screen for inspection and failure
reports. WT applies the same window behavior to every executable.

## Execution model

`start_window` returns after WT creates the Byobu window and starts its executable. The window
continues across client disconnects. `get_window` supports polling through the initial one-request
`wt api` client. A later client daemon can stream or reuse connections while preserving these
operations.

WT retains up to 64 MiB of output per window. It persists window metadata, status, cursors, and the
last screen in the registry. Retention removes complete oldest records and reports
`oldest_available` plus `output_gap` when a requested cursor was removed. Deleting the window
removes that state and closes its Byobu tab; deletion is idempotent at the guest boundary. Stopping
or deleting a world first stops its windows and records them as stopped.
