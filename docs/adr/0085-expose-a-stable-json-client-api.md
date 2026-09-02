# ADR 0085: Expose a stable JSON client API

- Status: Accepted
- Date: 2026-09-02

## Decision

The `wt api` command is WT's stable programmatic interface. It reads one versioned JSON request from
standard input and writes one JSON response to standard output. Diagnostics use standard error.
The request and response are UTF-8 JSON objects. Standard output contains only the response and its
trailing newline.

```json
{
  "api_version": 1,
  "request_id": "0199f65a-6758-7c13-818a-8e925b476d3e",
  "expected_server_id": "018efb7d-cf8b-70c1-a867-04e912f499a4",
  "context": "work",
  "operation": "create_world",
  "name": "review-auth",
  "vcpus": 2,
  "memory_mib": 4096,
  "disk_gib": 32,
  "git_user_name": "Ada Lovelace",
  "git_user_email": "ada@example.com"
}
```

Version 1 exposes `create_world`, `delete_world`, `start_window`, `get_window`,
`send_window_input`, `stop_window`, and `delete_window`. All operation fields are required.
`expected_server_id` is optional for a first request and binds later requests to the intended
server. Deletion identifies a world by `world_id` and means "ensure this world is absent", so
deleting a world that is already absent succeeds. ADR 0086 defines the window operations' fields,
lifecycle, authorization, cursor, and retention contracts.

The additional request shapes are:

```text
start_window(world_id, argv, cwd)
get_window(window_id, after, limit, include_screen)
send_window_input(window_id, control_token, data_base64)
stop_window(window_id, control_token)
delete_window(window_id, control_token)
```

They use the same `api_version`, `request_id`, optional `expected_server_id`, and `context` routing
fields as world operations. Their responses use the same `request_id`, `server_id`, tagged
`outcome`, structured error, and mutation-expiration envelope.

The command returns a tagged response:

```json
{
  "api_version": 1,
  "request_id": "0199f65a-6758-7c13-818a-8e925b476d3e",
  "server_id": "018efb7d-cf8b-70c1-a867-04e912f499a4",
  "expires_at_unix_ms": 1788374400000,
  "outcome": "ok",
  "result": {
    "world": {
      "world_id": "018f1e3d-95c0-7e46-8896-3d9b0abf62c8",
      "name": "review-auth",
      "status": "running",
      "vcpus": 2,
      "memory_mib": 4096,
      "disk_gib": 32,
      "guest_ip": "192.0.2.2",
      "ssh": {
        "user": "wt",
        "host": "192.0.2.2",
        "port": 22,
        "host_keys": ["ssh-ed25519 AAAA..."]
      }
    }
  }
}
```

An error response replaces `result` with `error`, containing a stable `code`, a human-readable
`message`, and `retryable`. Capacity errors also include structured resource details. All error
responses exit with a non-zero status. Local validation and transport failures still produce the
JSON error response and additionally write a diagnostic to standard error. A request is limited to
64 KiB.

The WT client resolves `context` through its existing configuration. It calls `wts api` locally or
over the context's SSH transport. SSH supplies the server identity and authorization boundary.

Each WT server creates a persistent `server_id`. On first use, clients trust the returned identity
and store it with WT resource IDs. They send it as `expected_server_id` on later requests. The
server rejects a mismatch before executing the operation. This keeps a renamed or redirected
client context from addressing the wrong server after that initial trust decision.

`api_version` versions the public contract. Version 1 may add optional response fields. Clients
ignore unknown response fields. WT rejects unknown request fields and unsupported versions. A
breaking request or response change requires a new API version. The public response uses dedicated
types rather than exposing control-protocol types directly. `wt` translates this contract to the
installed `wts` control protocol, which has its own version.

Every request has a client-generated UUID `request_id`. For a mutating operation, the server commits
each successful or non-retryable result under `(owner, request_id)`. Repeating the same semantic
operation returns that exact result after it is committed, including after a client disconnect or
server restart. Routing fields such as `context` and `expected_server_id` do not change an
operation's identity. Reusing the ID with different operation content returns a non-retryable
conflict. WT retains completed mutation results for 30 days and includes their expiration time in
the response. Retryable errors are not committed and omit the expiration time, allowing the same
request to be attempted again.

The server durably reserves the request ID before changing resource state. If the server stops after
a state change but before committing its API result, startup discards the incomplete result and
reconciles the resource. Retrying uses each operation's desired-state or durable identity behavior:
it does not create a duplicate world or window, enqueue the same input request twice, or fail merely
because the requested world is already absent, but it may return a new result rather than the
interrupted process's uncommitted response.

Responses echo `request_id` once it has been parsed and include `server_id` once a server responds.
Malformed requests may therefore omit both, and local configuration or transport failures omit
`server_id`.

## Initial execution model

Each request starts one `wt api` process. For an SSH context, that process starts one SSH command
and runs `wts api` on the server. `wts api` forwards the request to the WT server over its protected
Unix socket.

```text
controller -> wt api -> SSH -> wts api -> WT server
```

A later WT client daemon or connection pool can reuse processes and SSH connections while preserving
the same JSON request and response contract.
