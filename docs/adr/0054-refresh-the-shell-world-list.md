# ADR 0054: Refresh the shell world list

- Status: Accepted; Date: 2026-08-21

## Decision

Inventory refresh uses one dedicated `std::thread` which owns a cloned client
configuration. A stop-channel
`recv_timeout` starts the next refresh five seconds after the previous one
finishes and provides prompt cancellation between refreshes. Each context list
request has a one-minute timeout. Refreshes never overlap.

Successful complete snapshots cross to the UI thread through a one-slot MPSC
channel. A full channel drops the new snapshot, bounding backpressure; the next
refresh reads current state again. Failed or partial reads are not published.

The UI thread remains the sole owner of the model, PTYs, and terminal buffers;
the refresh thread never mutates UI state. World UUID is the reconciliation key
because names can be reused after deletion.

Dropping the refresh owner signals the stop channel and joins the thread.

## Consequences

Each shell consumes one refresh thread and a waiter thread during each request.
Managed SSH synchronization remains synchronous and has no explicit timeout.
