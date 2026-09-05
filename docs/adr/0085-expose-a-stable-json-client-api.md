# ADR 0085: Expose a stable JSON client API

- Status: Accepted
- Date: 2026-09-03

## Context

Controllers need a machine-readable world API independent of terminal UI text
and the internal client/server protocol.

## Decision

The `wt api` command exchanges one versioned UTF-8 JSON request and response over
stdio. Diagnostics use stderr. `api/api.ts` defines the public contract and generates
WT's Rust wire types. Consumers clone WT at a pinned commit and own their language
clients, schema generation, and validation.

Operations list contexts/worlds, create/delete worlds, read world mail, and execute
generic commands. Controllers own agent semantics
([ADR 0086](0086-controller-owned-agent-execution.md)).

Every request has a UUID. Responses to valid requests echo it; server-backed
responses identify the server and `expected_server_id` binds requests to a known
server. Local context discovery needs no server. Contexts resolve to local or
SSH transports.
Unknown request fields and versions are rejected; unknown response fields are ignored.

World mutations persist results by owner/request ID for 30 days. Same-content retries
return the recorded result; changed content conflicts. Read calls return current state.

`exec_world` is not a replayable world mutation. It checks ownership and running state,
then runs one absolute executable with argv and UTF-8 stdin as the guest user.
It has a 60-second timeout, 1 MiB input/argument limit, at most 256 arguments, and
16 MiB limits on each output stream. Nonzero command exit status is transport data.
Timeout or transport loss means unknown execution outcome, never permission to retry.
WT does not store command results or interpret agent requests carried in stdin.

The internal control protocol is separately versioned from the public JSON contract.
