# ADR 0054: Refresh the shell world list

- Status: Accepted; Date: 2026-08-21

## Decision

Inventory refresh uses one dedicated `std::thread`. Running `list_all` in the UI
timer path is rejected because one slow local or OpenSSH context would block
terminal input and rendering.

The refresh thread owns a cloned client configuration. A stop-channel
`recv_timeout` provides both the five-second cadence and prompt cancellation
between refreshes. Each iteration performs one `list_all` followed by managed
SSH synchronization, so refreshes never overlap.

Successful complete snapshots cross to the UI thread through an MPSC channel.
The UI loop drains the channel without blocking and uses only its newest value.
Failed or partial reads are not published.

The UI thread remains the sole owner of the model, PTYs, and terminal buffers;
the refresh thread never mutates UI state. World UUID is the reconciliation key
because names can be reused after deletion.

Dropping the refresh owner signals the stop channel and joins the thread.

## Consequences

Each shell consumes one additional thread. Shutdown can wait for an in-progress
list or SSH-sync call because the transport APIs are synchronous.
