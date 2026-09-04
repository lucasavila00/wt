# ADR 0085: Expose a stable JSON client API

- Status: Accepted
- Date: 2026-09-02

## Decision

The `wt api` command is WT's stable programmatic interface. It reads one versioned UTF-8 JSON
request from standard input and writes one JSON response plus a trailing newline to standard
output. Diagnostics use standard error.

Version 1 exposes these operations:

- `create_world` and `delete_world` manage a world by immutable `world_id`.
- `start_codex` starts an asynchronous Codex session in a visible Byobu window and returns its
  Codex thread ID, initial turn ID, and pane metadata.
- `inspect_codex` returns the thread's current status, active turn ID, pane metadata, and captured
  terminal screen.
- `send_codex_message` steers the active turn or starts the thread's next turn.
- `resume_codex` resumes a persisted thread and restores its visible window without starting a
  turn. It accepts `world_id` and `thread_id` and returns the inspection result shape. As a
  mutation, it uses request-ID replay; a later restart needs a new request ID.
- `read_world_mail` reads a bounded cursor page from a world's mailbox.

Deleting an already absent world succeeds. ADR 0084 defines mailbox behavior, and ADR 0086 defines
the Codex session lifecycle.

```json
{
  "api_version": 1,
  "request_id": "0199f65a-6758-7c13-818a-8e925b476d3e",
  "expected_server_id": "018efb7d-cf8b-70c1-a867-04e912f499a4",
  "context": "work",
  "operation": "start_codex",
  "world_id": "018f1e3d-95c0-7e46-8896-3d9b0abf62c8",
  "message": "Review the current change"
}
```

A response echoes `request_id`, identifies the WT server, and contains either `result` or an error
with a stable `code`, human-readable `message`, and `retryable` flag. Capacity errors also contain
structured resource details. Error responses exit nonzero. Local validation and transport failures
still produce JSON and additionally write a diagnostic to standard error. Requests are limited to
128 MiB.

The client resolves `context` through its existing configuration and calls `wts api` locally or
over SSH. Each server has a persistent `server_id`; `expected_server_id` binds later requests to the
server that owns previously returned world and Codex thread IDs.

`api_version` versions this public contract. Version 1 may add optional response fields. Clients
ignore unknown response fields; WT rejects unknown request fields and unsupported versions. The
public types remain separate from the internal control protocol.

Every request has a client-generated UUID. For a mutation, the server records each successful or
non-retryable result under `(owner, request_id)`. Repeating the same semantic operation returns the
committed result. Reusing the ID with different content is a non-retryable conflict. Routing fields
do not affect operation identity. Completed mutation results expire after 30 days; retryable errors
are eligible for another attempt. Read responses describe current state and do not carry mutation
expiration metadata.

Codex start and send responses acknowledge acceptance rather than waiting for a turn to finish.
`inspect_codex` reads the current guest runtime and terminal view. Terminal results arrive through
`read_world_mail`, which gives controllers a durable completion path after their live API
connection has ended.

Each invocation starts one `wt api` process. For SSH contexts it starts one SSH command, and
`wts api` forwards the request over WT's protected Unix socket. A future connection pool may reuse
transports without changing the JSON contract.
