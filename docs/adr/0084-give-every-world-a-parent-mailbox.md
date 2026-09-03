# ADR 0084: Let WT own Codex sessions and their parent mailbox

- Status: Proposed
- Date: 2026-09-03

## Context

An external controller needs to delegate work to several Codex sessions in one world, send later
inputs to a specific session, and receive progress and terminal results after disconnects or
restarts.

A world-only text mailbox does not identify which Codex session or turn produced a message. Making
the controller observe Codex App Server events or process exits would split session ownership
between WT and the controller. Making Byobu windows durable resources would instead expose WT's
terminal implementation and confuse a replaceable runtime object with a Codex session.

## Decision

WT owns Codex sessions that run in WT worlds. A Codex session is a durable, world-scoped resource
with a WT session ID and, after its first turn starts, the underlying persisted Codex thread ID. A
turn is a durable queued input with its own ID. WT serializes turns within a session.

WT initially executes each turn with the non-interactive Codex CLI. The first turn uses `codex
exec --json <MESSAGE>`; later turns use `codex exec resume --json <CODEX_THREAD_ID> <MESSAGE>`. WT
consumes the JSONL internally to learn the Codex thread ID and terminal outcome. The Codex process
is ephemeral and is not a WT resource.

The stable `wt api` transport from ADR 0085 will expose these operations:

- `create_codex_session(world_id, message)` creates a session, enqueues its first turn, and returns
  the WT session and turn IDs;
- `send_to_codex_session(session_id, message)` enqueues another turn and returns its turn ID; and
- `read_world_mail(world_id, after_message_id, limit)` returns an ascending bounded page and the
  read's high-water message ID.

Creating a session or enqueueing a turn returns after the durable resource and input exist; it does
not wait for Codex. These mutations use the API request idempotency contract from ADR 0085.

Every world has a durable outbound mailbox in `wts`. A mailbox row contains a monotonic server
message ID, world ID, WT Codex session ID, turn ID when applicable, creation time, kind, and text.
The initial kinds are:

- `message` for an explicit `send_message_to_parent` update during a turn;
- `completed` for the final assistant message from a successful turn;
- `failed` for a turn that ended with a Codex or execution error; and
- `interrupted` when WT stopped tracking an in-flight turn before it obtained a terminal result.

WT appends exactly one terminal mailbox entry for every accepted turn. It commits that entry before
making the session idle and starting its next queued turn. A uniqueness constraint on the turn ID
prevents duplicate terminal entries during recovery. The model does not need to call
`send_message_to_parent` for its final result.

WT gives each active Codex turn an unguessable mailbox capability through its process environment.
The guest tool gateway validates that capability and derives the world, session, and turn rather
than accepting those identities from the caller. Any code in that Codex process may use the
capability, matching the world's existing guest trust boundary. The capability expires with the
turn and is not durable session state.

Mailbox reads are owner-scoped, ascending, and bounded by both entry count and a 32 MiB response
budget. They include a high-water message ID so a consumer can finish a finite scan while new
messages arrive. Consumers checkpoint a cursor only after committing the returned entries.
Retrying a read may therefore repeat entries; the stable server message ID is the deduplication
key.

Session inputs and mailbox text are limited to 16 MiB each. WT retains the complete final assistant
message or error within that bound; it does not impose a small aggregate mailbox quota or silently
truncate terminal results. An oversized terminal result produces a bounded `failed` entry rather
than a partial `completed` entry. Outputs that do not reasonably fit in 16 MiB of text belong in a
future artifact resource rather than in the mailbox.

Deleting a world first prevents further session input and writes an `interrupted` terminal entry
for every accepted turn that cannot finish. Once no producer can append more mail, deletion returns
the mailbox's final high-water message ID. It does not delete the WT Codex session records or
mailbox entries. Those records become historical and remain readable by their immutable world and
session IDs. Their owner remains recorded so reads stay owner-scoped after the live world record is
gone. WT will not silently expire them. A separate, explicit retention or deletion decision may be
added when storage management is actually required.

Codex rollout files remain durable and isolated per world under ADR 0082. WT does not promise to
resume an executing process after a WT restart. It records the active turn as `interrupted`; a
later input resumes the persisted Codex session in a new process.

Byobu windows and panes remain human terminal runtime state. They are not session identifiers,
mailbox routes, output stores, or API resources. A future human-facing command may open or resume a
WT-owned Codex session in the TUI without changing this control-plane contract.

## Consequences

- Controllers consume WT session and mailbox resources without speaking the Codex protocol.
- Explicit progress and automatic terminal results share one ordered durable mailbox.
- Mailbox history remains available after its world is deleted.
- Multiple Codex sessions can share a world without using windows for identity.
- WT is intentionally coupled to the Codex CLI session and JSONL contracts.
- No Codex process, SSH stream, Byobu window, pane, or terminal output becomes durable WT state.
