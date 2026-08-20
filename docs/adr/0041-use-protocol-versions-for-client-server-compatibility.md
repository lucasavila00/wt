# ADR 0041: Use protocol versions for client-server compatibility

- Status: Accepted
- Date: 2026-08-17
- Supersedes: [ADR 0007](0007-require-matching-client-and-server-commits.md)
- Amended by: [ADR 0043](0043-own-retained-guest-foundation-in-shared-images.md)

## Context

WT rejects every client and server built from different Git commits. This
detects incompatible control-plane changes before an operation can mutate
state, but it also rejects builds whose wire protocol is identical. During
development, unrelated client or server changes therefore require both
binaries to be reinstalled together.

The control-plane request and response envelopes already carry a protocol
version. Both sides validate that version before relying on an operation or
response.

## Decision

The protocol version is the client-server compatibility boundary. Clients and
servers with the same protocol version may communicate even when they were
built from different Git commits.

Requests do not carry a client Git commit. The server rejects an unsupported
request protocol version before executing the operation, and the client
rejects a response with an unsupported protocol version.

Change `PROTOCOL_VERSION` whenever a request, response, or operation semantic
becomes incompatible with existing clients or servers. Compatible changes do
not require a protocol version change.

## Consequences

- Unrelated client and server changes no longer require coordinated installs.
- Incompatible versions still fail before an operation executes.
- Developers must identify incompatible protocol changes and increment the
  protocol version deliberately.
- Git commit hashes are no longer part of the control-plane wire contract or
  embedded solely to enforce client-server compatibility.
