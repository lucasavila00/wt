# ADR 0084: Give every world a parent mailbox

- Status: Accepted
- Date: 2026-09-02

## Decision

Every world has a durable outbound mailbox in `wts`. Guest software sends an untargeted message with:

```json
{"command":{"action":"send_message_to_parent","message":"..."}}
```

The gateway authenticates only the source world by resolving the VSOCK peer CID to an active WT
domain. This is the trust boundary: any software in that guest can send a message for that world.
WT records no process, Byobu, tmux, or window attribution.

A mailbox row contains only a monotonic server message ID, world ID, creation time, and message.
The gateway commits the row before returning success. A lost response followed by a retry may create
a duplicate message. This is an at-least-once human notification, not an exactly-once command.

Messages are limited to 64 KiB; a world retains at most 64 MiB. A full mailbox rejects new mail.
Deleting a world cascades its mailbox rows. Existing agent-tool reports and their data are unchanged.

`wts` provides an owner-scoped bounded cursor read for the built-in `wt messages` command:
`list_world_mail(world_id, after_id, limit)`. It is an internal control-plane operation, not part
of the stable public JSON API. It returns ascending IDs and a high-water ID so the client can finish
a bounded scan while messages arrive.

## Consequences

- Mail survives client, guest, and server restarts after commit.
- Mail is associated with a world only.
- `wtg tools` reports remain available alongside parent messages.
