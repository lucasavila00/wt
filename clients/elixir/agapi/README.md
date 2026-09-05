# Agapi Elixir client

```elixir
client = Agapi.Local.client(state_dir: "/absolute/agapi-state")
Agapi.call(client, %{
  "request_id" => "11111111-1111-4111-8111-111111111111",
  "operation" => "new_thread",
  "message" => "Change the home page hero text"
})
```

Alternatively, `Agapi.new(fn json -> ... end)` accepts a transport returning
`{:ok, %{stdout: bytes, stderr: bytes, exit_status: integer}}` or `{:error, reason}`.
The client checks response version, request identity and outcome. It never retries
transport failures. No dependency on WT, containers, or Codex is present.
