# ADR 0084: Let WT run Codex turns and own the parent mailbox

- Status: Accepted
- Date: 2026-09-03

## Context

An external controller needs to delegate work to Codex in a world and recover the final result
after a client connection disappears. WT already has world lifecycle serialization, Codex access,
and a durable world mailbox.

## Decision

WT exposes one blocking operation, `run_codex_turn(world_id, session_id?, message)`. An initial call
runs `codex exec --json`; a call carrying a session ID runs `codex exec resume --json`. The returned
Codex thread UUID is an opaque session ID that a caller may pass to a later turn. Execution state
lives for the duration of the blocking call.

WT holds the existing per-world operation lock for the whole turn and mailbox write. A second turn
or lifecycle operation in that world receives a retryable conflict; other worlds can proceed in
parallel. The server handler continues the operation after a client disconnect when the underlying
server process remains alive.

Every terminal result is appended before WT returns. It uses the existing `world_mail.message`
column with a private versioned JSON envelope containing the API request ID, optional session ID,
`completed` or `failed` kind, and full text. Mail reads decode that envelope; old and explicit guest
messages remain ordinary unattributed `message` entries. A controller can therefore recover an
unknown response by reading mail and matching its request ID.

Mail reads remain owner-scoped, ascending, bounded by entry count, and cursor-based. Their
high-water ID gives consumers a finite scan boundary. The stable message ID is the deduplication
key. The existing `send_message_to_parent` tool remains a simple world-scoped write.

Inputs and complete stored mailbox rows have a coarse 64 MiB UTF-8 limit. The transport accepts
requests up to 128 MiB.

Mailbox rows retain the existing foreign key to their world and are deleted with it. Consumers
import required messages before deleting a world. If `wts` stops during a turn, retrying can repeat
work; this is the initial crash behavior of the blocking operation.

## Consequences

- Controllers use a small WT-owned execution and mailbox API.
- Terminal messages are durable and execution state lasts for one blocking call.
- WT is coupled to the non-interactive Codex CLI thread and JSONL contracts.
- A server crash can make retrying a turn repeat work.
