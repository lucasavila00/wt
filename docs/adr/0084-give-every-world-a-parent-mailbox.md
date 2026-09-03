# ADR 0084: Let WT run Codex turns and own the parent mailbox

- Status: Accepted
- Date: 2026-09-03

## Context

An external controller needs to delegate work to Codex in a world and receive the final result even
if the client connection disappears. Byobu windows are ephemeral human UI state, not a reliable
automation API. Making sessions, turns, queues, or windows durable WT resources would add lifecycle
and recovery machinery that the first API does not need.

## Decision

WT exposes one blocking operation, `run_codex_turn(world_id, session_id?, message)`. Without a
session ID WT runs `codex exec --json`; with one it runs `codex exec resume --json`. The returned
Codex thread UUID is an opaque session ID that a caller may pass to a later turn. WT does not keep a
session or turn resource, queue inputs, or persist execution state.

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
key. The existing `send_message_to_parent` tool remains a simple world-scoped write without process
attribution.

Inputs and stored mailbox rows have a coarse 64 MiB UTF-8 limit. The transport accepts requests up
to 128 MiB. WT neither imposes an aggregate mailbox quota nor silently truncates messages.

Mailbox rows retain the existing foreign key to their world and are deleted with it. WT provides no
post-deletion history. If `wts` stops during a turn, the guest Codex process may continue without a
terminal mailbox row; retrying can repeat work. Queueing and crash recovery can be added only if
real use proves they are needed.

Byobu windows and panes remain replaceable runtime presentation. They are not session identifiers,
mailbox routes, output stores, or API resources.

## Consequences

- Controllers use a small WT-owned execution and mailbox API without speaking Codex JSONL.
- Only terminal messages are durable; Codex processes and session lifecycle are not.
- WT is coupled to the non-interactive Codex CLI thread and JSONL contracts.
- The simple design deliberately accepts possible duplicate work across a server crash.
