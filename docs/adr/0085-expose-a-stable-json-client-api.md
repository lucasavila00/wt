# ADR 0085: Expose a stable JSON client API

The wt api command exchanges one versioned UTF-8 JSON request and response over
stdio. Diagnostics use stderr. The typed Elixir client accepts an injected transport;
its default starts wt api. TypeScript/Beff defines the generated contract.

Operations list contexts/worlds, create/delete worlds, read world mail, and execute
generic commands. Agent semantics belong to agapi (ADR 0086).

Every request has a UUID. Responses echo it and identify the server; expected_server_id
binds requests to a known server. Contexts resolve to local or SSH transports.
Unknown request fields and versions are rejected; unknown response fields are ignored.

World mutations persist results by owner/request ID for 30 days. Same-content retries
return the recorded result; changed content conflicts. Read calls return current state.

exec_world is not a replayable world mutation. It checks ownership and running state,
then runs one absolute executable with argv and UTF-8 stdin as the guest user.
It has a 60-second timeout, 1 MiB input/argument limit, at most 256 arguments, and
16 MiB limits on each output stream. Nonzero command exit status is transport data.
Timeout or transport loss means unknown execution outcome, never permission to retry.
WT does not store command results or interpret agent requests carried in stdin.

The internal control protocol is separately versioned from the public JSON contract.
