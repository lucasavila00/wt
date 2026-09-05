# ADR 0084: Give every world a parent mailbox

- Status: Accepted
- Date: 2026-09-03

## Context

Work in a world needs a durable path back to its controller that survives a
client disconnect and does not depend on live terminal observations.

## Decision

Every world has one durable, owner-scoped outbound mailbox. The `wtg tools`
command `send_message_to_parent` appends UTF-8 text. The gateway derives the
source world from the accepted vsock peer's active WT libvirt domain; the
caller supplies no world, window, or process identity.

Writes commit before acknowledgement. Messages have a monotonic ID, world ID,
creation time, and text. Reads are ascending and cursor-based, returning the
high-water ID observed at the start of the read. Consumers use message IDs for
deduplication and a captured high-water ID to bound a scan while mail arrives.
A message is limited to 64 MiB of UTF-8 text; a read returns at most 1,000 entries.

Agent execution and terminal result delivery belong to controllers
([ADR 0086](0086-controller-owned-agent-execution.md)).
WT does not automatically append Codex completion results. The wire contract
and gateway retain support for Codex result envelopes and their identity
fields, but that support does not provide an agent supervisor or completion
service.

## Consequences

Controllers can consume guest messages incrementally and idempotently.
Mailbox rows are deleted with their world; controllers must import messages
they need to retain before deleting it.
