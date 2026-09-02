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
  "request_id": "0199...",
  "context": "work",
  "operation": "list_world_mail",
  "world_id": "018f...",
  "after_id": 120,
  "limit": 100
}
```

The command returns a tagged response:

```json
{
  "api_version": 1,
  "request_id": "0199...",
  "server_id": "018e...",
  "outcome": "ok",
  "result": {"messages": []}
}
```

An error response replaces `result` with `error`, containing a stable `code`, a human-readable
`message`, and `retryable`. Server rejections are JSON responses. Local validation and transport
failures exit with a non-zero status and write diagnostics to standard error.

The WT client resolves `context` through its existing configuration. It calls `wts api` locally or
over the context's SSH transport. SSH supplies the server identity and authorization boundary.

Each WT server creates a persistent `server_id`. Clients store it with WT resource IDs and verify it
after resolving a context. This keeps a renamed or redirected client context from addressing the
wrong server.

`api_version` versions the public contract. Version 1 may add optional response fields. Clients
ignore unknown response fields. WT rejects unknown request fields and unsupported versions. A
breaking request or response change requires a new API version. `wt` translates this contract to
the installed `wts` control protocol, which has its own version.

Every request has a client-generated `request_id`. For a mutating operation, `wts` commits the
result under `(owner, request_id)`. Repeating the same request returns that result. Reusing the ID
with different request content returns a conflict error. WT retains mutation results for 30 days
and includes their expiration time in the response.

## Initial execution model

Each request starts one `wt api` process. For an SSH context, that process starts one SSH command
and runs `wts api` on the server. `wts api` forwards the request to the WT server over its protected
Unix socket.

```text
controller -> wt api -> SSH -> wts api -> WT server
```

A later WT client daemon or connection pool can reuse processes and SSH connections while preserving
the same JSON request and response contract.
