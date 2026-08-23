# ADR 0031: Refresh the shell world list

- Status: Accepted; Date: 2026-08-21

## Decision

Inventory refresh uses one dedicated `std::thread` which owns a cloned client
configuration. A stop-channel `recv_timeout` starts the next refresh five
seconds after the previous one
finishes and provides prompt cancellation between refreshes. Each context list
request has a one-minute timeout. Refreshes never overlap.

Successful complete snapshots cross to the UI thread through a one-slot MPSC
channel. A full channel drops the new snapshot, bounding backpressure; the next
refresh reads current state again. Failed or partial reads are not published.

Snapshots carry a generation. Starting or completing an in-shell creation
advances it, so a list started before that mutation cannot overwrite its SSH
configuration or model state. SSH configuration is written only for the exact
snapshot accepted by the UI; a write failure leaves the UI unchanged.

World identity is `(context, UUID)`; names can be reused and one server can be
configured through multiple contexts. Session reader events use local monotonic
tokens so removed readers cannot address replacement sessions.
Closed SSH sessions remain attached to their world until the user reconnects;
inventory refresh does not silently replace them.
Timed helpers run in a dedicated process group while separate threads drain
stdout and stderr. On interruption, the group is killed and the helper reaped.
Cancellation is checked during a request and between contexts before joining
the worker.
Each shell consumes one refresh thread and two pipe-reader threads per request.
