# ADR 0084: Give every world a parent mailbox

- Status: Accepted
- Date: 2026-09-03

## Context

Work performed in a world needs a small durable path back to its controller. Live Codex session
events are useful while a session is running, but a terminal result also needs to survive a client
disconnect and remain available after the session window has closed.

## Decision

Every world has one durable, owner-scoped outbound mailbox. The guest command
`send_message_to_parent` appends a UTF-8 message to that mailbox. The gateway derives the source
world from the authenticated guest connection, so callers do not supply a world ID.

Mailbox rows contain a monotonic message ID, world ID, creation time, and message. Reads are
ascending and cursor-based, accept a bounded count, and return the high-water ID observed at the
start of the read. A consumer can therefore finish a finite scan while newer messages arrive and
use the message ID as its import deduplication key.

ADR 0086 uses the same mailbox for terminal Codex-session delivery. WT appends one terminal entry
when a turn completes or fails. Its versioned message payload carries the Codex thread ID, turn ID,
pane ID, terminal status, and final assistant or error text so the controller can associate the
result with the session it started. The payload is a delivery record; live window, App Server, and
turn state remain runtime state.

Mailbox writes commit before they are acknowledged. Explicit guest messages and terminal session
messages share the same ordering and read path. A message is limited to 64 MiB of UTF-8 text, and a
read returns at most 1,000 entries.

Mailbox rows retain a foreign key to their world and are deleted with it. A controller imports
messages through the deletion high-water mark before deleting a world when it needs to retain
them.

## Consequences

- Guest tools have one small, durable way to report to the parent controller.
- Controllers consume mail incrementally and idempotently.
- Codex terminal results remain available independently of the live session connection.
- World deletion is also the mailbox retention boundary.
