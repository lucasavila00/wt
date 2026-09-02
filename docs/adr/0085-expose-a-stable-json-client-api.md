# ADR 0085: Expose a stable JSON client API

- Status: Accepted
- Date: 2026-09-02

## Decision

The `wt api` command is WT's stable programmatic interface. It reads one versioned JSON request
from standard input and writes exactly one JSON response to standard output. Diagnostics use
standard error.

`api_version` versions the public contract. `wt` translates this contract to the installed `wts`
control protocol, which has its own version. This keeps API compatibility separate from internal
client-server compatibility.

Version 1 accepts only `create_world` and `delete_world`. Every request includes `api_version: 1`
and a configured `context`. Requests are limited to 64 KiB. Unknown fields are rejected.

```json
{
  "api_version": 1,
  "context": "work",
  "operation": "create_world",
  "name": "agent-1",
  "vcpus": 2,
  "memory_mib": 4096,
  "disk_gib": 32,
  "git_user_name": "Ada Lovelace",
  "git_user_email": "ada@example.com"
}
```

```json
{
  "api_version": 1,
  "context": "work",
  "operation": "delete_world",
  "world_id": "018f..."
}
```

Successful creation returns `{"api_version":1,"outcome":"ok","response":{"response":"world","world":...}}`.
The `world` object has `world_id`, `name`, `status`, `vcpus`, `memory_mib`, and `disk_gib` fields.
It may also contain `guest_ip`, `last_error`, and `ssh`; the `ssh` object has `user`, `host`,
`port`, and `host_keys`. `status` is a snake-case string. Successful deletion returns
`{"api_version":1,"outcome":"ok","response":{"response":"world_deleted","world_id":"..."}}`.

Responses are open objects: a later compatible WT release may add fields, and clients must ignore
fields they do not understand. Clients must also tolerate future `status` values. The `outcome`,
`response`, `code`, and `details.kind` discriminators are closed for API version 1.

Every rejected request returns
`{"api_version":1,"outcome":"error","error":{"code":"...","message":"..."}}`.
Successful responses exit with status zero; rejected requests exit with status one.
The stable error codes are `invalid_request`, `unsupported_api_version`, `configuration_error`,
`unknown_context`, `context_error`, `unsupported_protocol`, `conflict`, `not_found`, `capacity`,
`backend_error`, and `internal_error`.

`message` is diagnostic text and is not machine-stable. A `capacity` error includes
`details: {"kind":"capacity","resource":"cpu"|"memory"|"disk","total":...,
"reserved":...,"requested":...}` for programmatic handling. Other error codes have no `details`
field in version 1.

`delete_world` is idempotent: it means “ensure this world ID is absent.” Deleting an already absent
or unknown ID returns the successful `world_deleted` response with the requested ID. Creation uses
the world name and complete creation inputs as its idempotency identity. Retrying that identical
request returns the existing world while it is provisioning or running; a changed input or a world
in any other state returns `conflict`.

The WT client resolves `context` through its existing configuration. It calls `wts api` locally or
over the context's SSH transport. SSH supplies the server identity and authorization boundary.

## Initial execution model

Each request starts one `wt api` process. For an SSH context, that process starts one SSH command
and runs `wts api` on the server. `wts api` forwards the request to the WT server over its protected
Unix socket.

```text
controller -> wt api -> SSH -> wts api -> WT server
```

A later WT client daemon or connection pool can reuse processes and SSH connections while preserving
the same JSON request and response contract.

When standard input can be read and standard output is writable, `wt api` emits one newline-delimited
response envelope. Failures reading standard input or writing standard output are process failures
and cannot reliably produce that envelope.
