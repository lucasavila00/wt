# ADR 0084: Give every world a parent mailbox

- Status: Accepted
- Date: 2026-09-02

## Decision

Every WT world has an outbound mailbox in `wts`. A process in a WT-managed Byobu window sends a
message with the always-available `wtg tools` command:

```json
{"command":{"action":"send_message_to_parent","message":"..."}}
```

This command is the single untargeted `wtg tools` operation. It replaces
`report_wt_tool_bug`, `report_wt_tool_issue`, `suggest_wt_tool_improvement`, and
`request_wt_tool_feature`.

Codex invokes the command through its shell tool. Other programs in managed windows use the same
command. `wtg tools` creates a client message ID and reuses it for transport retries.

## Transport and storage

`wtg tools` sends the request to the guest relay over its Unix socket. The relay obtains the caller
PID from Unix peer credentials and derives its WT window from WT-managed process membership. The
relay sends the native tmux window identity and message to the host gateway over VSOCK. The gateway
authenticates the source world from the VSOCK peer CID and active libvirt domain.

The source world is the host-enforced trust boundary. The stock relay's process lookup supplies
cooperative window provenance, not a host-authenticated process identity: software inside the VM can
bypass the relay and send a native tmux window ID directly over VSOCK. The gateway verifies that the
ID names a currently managed window in the authenticated world, but it cannot prove which guest
process submitted it. Controllers must not use the mailbox `window_id` as an authorization identity.

The gateway commits the message before it returns success. A mailbox row contains the server message
ID, client message ID, world ID, window ID, creation time, and message. The server message ID is
monotonic within one WT server. The unique `(world_id, window_id, client_message_id)` key makes a
retried send return the original row.

`window_id` is the globally unique immutable UUID defined by ADR 0086. The stock guest-to-host relay
uses the caller-derived native tmux window ID; after authenticating the VSOCK world, the gateway
resolves `(world_id, tmux_window_id)` through the durable managed-window registry before inserting
the mailbox row. This validation and insertion occur under the registry's write serialization.

The mailbox row deliberately does not reference the managed-window row with a foreign key. A send
must name a currently managed window, but committed mail remains readable after that window exits.

The `wts` control API exposes an owner-scoped cursor query:

```text
list_world_mail(world_id, after_id, limit)
```

Results use ascending ID order. `after_id` is exclusive. Repeating a page is safe because each row
keeps its stable server message ID. Each response includes the highest committed message ID observed
when the query began. A client can drain through that high-water ID while new messages arrive.
Reading preserves the mailbox contents.

Messages are limited to 64 KiB. Each world retains up to 64 MiB of mailbox data. A full mailbox
rejects new messages with a capacity error. Deleting a world deletes its mailbox rows.

## User interface

`wt ls` and world cards show the total retained message count for each world. The shell provides a
mailbox view with window, message time, and text. `wt messages` lists the same server-backed mail
across configured contexts.

The mailbox UI reads the same control-plane query used by external clients. Messages remain visible
after an external controller reads them.

## Replacement

The implementation replaces `agent_tool_reports` with world mailbox storage and replaces the
report kind, report list, report clear, and report count APIs with mailbox records and cursor reads.
Generated `wtg tools` types and help expose `send_message_to_parent` as the fixed guest command.

## Consequences

- A message survives guest, Codex, client, and `wts` process restarts after the registry commit.
- External controllers route messages by server, world, and window.
- WT authenticates the source world. Window attribution is validated cooperative provenance, not an
  authorization boundary. Controllers own parent relationships.
- The mailbox has one behavior for every world.
