# WT API for Elixir

`wt_api` is WT's typed Elixir client for the version 1 `wt api` JSON protocol.
It is consumed directly from the WT Git repository and is not published to Hex.

```elixir
def deps do
  [
    {:wt_api,
     git: "https://github.com/lucasavila00/wt.git",
     sparse: "clients/elixir/wt_api",
     ref: "<pinned commit>"}
  ]
end
```

Callers provide request IDs so they can preserve identity across their own
workflows:

```elixir
request = %WtApi.Request.CreateWorld{
  request_id: "11111111-1111-4111-8111-111111111111",
  context: "production",
  name: "review-42",
  vcpus: 2,
  memory_mib: 4096,
  disk_gib: 32,
  git_user_name: "Ada Lovelace",
  git_user_email: "ada@example.com"
}

{:ok, %WtApi.Success{server_id: server_id, result: result}} = WtApi.create_world(request)
world = result.world
```

The client invokes `wt api` directly through Exile with request JSON on stdin.
Standard error remains separate from the JSON response; no shell or temporary
file participates in transport. Failures are returned as `WtApi.TransportError`,
`WtApi.ProtocolError`, or `WtApi.ServerError` structs.

## Contract generation

[`api.ts`](api.ts) is the canonical API contract. Beff turns it into
[`wt_api.schema.json`](wt_api.schema.json), then the package generators produce
the checked-in Elixir request, result, and decoder modules and WT's Rust wire
types.

From the WT repository root:

```console
npm run generate:elixir-client
make check-elixir-client
```
