# ADR 0084: Give every world a parent mailbox

- Status: Accepted
- Date: 2026-09-02

## Decision

Every WT world has an outbound mailbox in `wts`. A process in the world sends a message with the
always-available `wtg tools` command:

```json
{"command":{"action":"send_message_to_parent","message":"..."}}
```

This command is the single untargeted `wtg tools` operation. It replaces
`report_wt_tool_bug`, `report_wt_tool_issue`, `suggest_wt_tool_improvement`, and
`request_wt_tool_feature`.

Codex invokes the command through its shell tool. Other programs in the world use the same command.
World creation always provides the guest relay, command schema, and mailbox path.

## Transport and storage

`wtg tools` sends the request to the guest relay over its Unix socket. The relay sends it to the
host gateway over VSOCK. The gateway derives the source world from the VSOCK peer CID and active
libvirt domain.

The gateway commits the message to the WT registry before it returns success. Mailbox rows contain
a monotonic ID, world ID, creation time, and message. Deleting a world deletes its mailbox rows.

The `wts` control API exposes an owner-scoped cursor query:

```text
list_world_mail(world_id, after_id, limit)
```

Results use ascending ID order. `after_id` is exclusive. Repeating a page is safe because each row
keeps its stable WT message ID. Reading preserves the mailbox contents.

## User interface

`wt ls` and world cards show each world's mailbox count. The shell provides a mailbox view for the
selected world with message time and text. `wt messages` lists the same server-backed mail across
configured contexts.

The mailbox UI reads the same control-plane query used by external clients. Messages remain visible
after an external controller reads them.

## Replacement

The implementation replaces `agent_tool_reports` with world mailbox storage and replaces the
report kind, report list, report clear, and report count APIs with mailbox records and cursor reads.
Generated `wtg tools` types and help expose `send_message_to_parent` as the fixed guest command.

## Consequences

- A message survives guest, Codex, client, and `wts` process restarts after the registry commit.
- External controllers poll each world mailbox and route messages using their own relationships.
- WT authenticates the sender as a world. External controllers own parent relationships.
- The mailbox has one behavior for every world.
