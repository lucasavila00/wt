# ADR 0085: Expose a stable JSON client API

- Status: Accepted
- Date: 2026-09-02

## Decision

The `wt api` command is WT's stable programmatic interface. It reads one versioned UTF-8 JSON
request from standard input and writes one JSON response plus a trailing newline to standard
output. Diagnostics use standard error.

Version 1 exposes only `create_world`, `delete_world`, `run_codex_turn`, and `read_world_mail`.
`run_codex_turn` identifies a world, supplies a message, and optionally supplies the opaque session
ID returned by an earlier turn. It blocks until WT has written the terminal mailbox entry. Mail
reads identify a world, cursor, and count limit. Deletion identifies a world by immutable
`world_id` and means "ensure this world is absent," so deleting an already absent world succeeds.
ADR 0084 defines Codex execution and mailbox behavior.

```json
{
  "api_version": 1,
  "request_id": "0199f65a-6758-7c13-818a-8e925b476d3e",
  "expected_server_id": "018efb7d-cf8b-70c1-a867-04e912f499a4",
  "context": "work",
  "operation": "run_codex_turn",
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
server that owns previously returned resource IDs.

`api_version` versions this public contract. Version 1 may add optional response fields. Clients
ignore unknown response fields; WT rejects unknown request fields and unsupported versions. The
public types remain separate from the internal control protocol.

Every request has a client-generated UUID. For a mutation, the server records each successful or
non-retryable result under `(owner, request_id)`. Repeating the same semantic operation returns the
committed result. Reusing the ID with different content is a non-retryable conflict. Routing fields
do not affect operation identity. Completed results expire after 30 days; retryable errors are not
committed.

If the server stops before committing a result, startup discards the incomplete request record.
Create and delete remain safe desired-state retries. A Codex turn is different: its terminal mail
can be recovered by request ID if it was written, but a crash before that write can require a retry
and duplicate work.

Each invocation currently starts one `wt api` process. For SSH contexts it starts one SSH command,
and `wts api` forwards the request over WT's protected Unix socket. A future connection pool may
reuse transports without changing the JSON contract.
