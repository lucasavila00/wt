# ADR 0085: Expose a stable JSON client API

- Status: Accepted
- Date: 2026-09-02

## Decision

The `wt api` command is WT's stable programmatic interface. It reads one versioned JSON request from
standard input and writes one JSON response to standard output. Diagnostics use standard error.

```json
{
  "api_version": 1,
  "context": "work",
  "operation": "list_world_mail",
  "world_id": "018f...",
  "after_id": 120,
  "limit": 100
}
```

The WT client resolves `context` through its existing configuration. It calls `wts api` locally or
over the context's SSH transport. SSH supplies the server identity and authorization boundary.

`api_version` versions the public contract. `wt` translates this contract to the installed `wts`
control protocol, which has its own version. This keeps API compatibility separate from internal
client-server compatibility.

## Initial execution model

Each request starts one `wt api` process. For an SSH context, that process starts one SSH command
and runs `wts api` on the server. `wts api` forwards the request to the WT server over its protected
Unix socket.

```text
controller -> wt api -> SSH -> wts api -> WT server
```

A later WT client daemon or connection pool can reuse processes and SSH connections while preserving
the same JSON request and response contract.
