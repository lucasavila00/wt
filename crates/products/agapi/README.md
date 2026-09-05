# agapi

Independent agent API, currently backed by Codex. It shares this repository's
build tooling, not WT's runtime or release lifecycle. Neither wtg nor wts links it.

## Run locally or inside an existing world

```sh
cargo build --release -p agapi
target/release/agapi --state-dir /absolute/agapi-state serve \
  --workspace /absolute/workspace \
  --codex /absolute/codex
```

Keep `serve` running under a process supervisor. It validates the exact version in
`codex-version`, owns the Codex child, and reconciles terminal results.
Use separate state directories for separate execution environments.
Local execution uses the host user's permissions. agapi does not provide isolation.

```sh
printf '%s\n' '{"api_version":1,"request_id":"11111111-1111-4111-8111-111111111111","operation":"new_thread","message":"Change the home page hero text"}' |
  target/release/agapi --state-dir /absolute/agapi-state api
```

Each call reads one JSON request and writes one JSON response. Responses echo
`api_version` and `request_id`, with either `outcome: "ok", result` (exit 0)
or `outcome: "error", error` (nonzero). No shell command parsing is involved.

Operations:

| Operation | Fields beyond version/request ID |
| --- | --- |
| describe | none; verifies the runtime and reports canonical workspace/state paths |
| new_thread | message |
| inspect_thread, resume_thread | thread_id |
| send_message | thread_id, message |
| steer_turn | thread_id, turn_id, message |
| interrupt_turn | thread_id, turn_id |
| read_events | after (cursor), limit (1–100, default 100) |
| ack_events | through (cursor) |

Thread and turn IDs belong to agapi, not the provider. Mutations record their
request before dispatch. Reusing an ID returns a stored result or an explicit
unknown outcome; it never repeats an uncertain submission.
Resuming does not submit a prompt. Unloaded unfinished turns become recovery
failures rather than being replayed.

The durable outbox assigns monotonic event IDs. Consumers commit events and their
cursor before acknowledging. Acknowledgement is monotonic; events remain readable
for recovery. A single controller owns acknowledgements for each state directory.

The Elixir package at `clients/elixir/agapi` accepts an injected JSON transport.
`Agapi.Local` runs the real executable. WT transport composition belongs to the
caller, not this package.

## Independent updates

`scripts/install-agapi VERSION` installs a published `agapi-vVERSION` release and
its matching Codex CLI. Stop `serve`, update, and restart it against the same
state directory. No WT release, world rebuild, or provider-specific WT API is
required. The installer keeps its Codex binary at
`~/.local/share/agapi/codex/bin/codex`; pass this path to `serve --codex`.
It does not replace the interactive `codex` command, alter shell profiles, or
overwrite Codex configuration or credentials. At runtime, Codex still uses the
normal `~/.codex` home, including WT's shared authentication and session mounts.

## Verify

```sh
AGAPI_CODEX_TEST_BINARY=/absolute/codex cargo test -p agapi -- --include-ignored
```

The integration test uses real Codex with a localhost-only Responses fixture.
