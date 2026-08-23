# ADR 0017: Use protocol versions for client-server compatibility

- Status: Accepted
- Date: 2026-08-17

## Context

The control-plane request and response envelopes carry a protocol version.
Both sides must validate that version before relying on an operation or
response.

## Decision

The protocol version is the client-server compatibility boundary. Clients and
servers with the same protocol version may communicate.

The server rejects an unsupported request protocol version before executing
the operation, and the client rejects a response with an unsupported protocol
version.

Change `PROTOCOL_VERSION` whenever a request, response, or operation semantic
becomes incompatible with existing clients or servers. Compatible changes do
not require a protocol version change.

## Consequences

- Incompatible versions still fail before an operation executes.
- Developers must identify incompatible protocol changes and increment the
  protocol version deliberately.
