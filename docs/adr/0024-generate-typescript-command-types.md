# ADR 0024: Generate TypeScript command types

- Status: Accepted
- Date: 2026-08-21

## Context

`wt-tools help` needs a readable TypeScript command contract. Maintaining the
same variants and fields separately in Rust lets the two definitions drift.

## Decision

Keep readable, commented TypeScript declarations as the source of truth.
Include them verbatim in `wt-tools help` and check them with pinned TypeScript
and Prettier through the repository npm workspace.

Use SWC in the `wt-tools` build script to parse the declarations and generate
Rust types into `OUT_DIR`. Support only the declarations used by the contract:
exported aliases, string-literal and object unions, primitives, references,
arrays, and optional fields. Reject other constructs.

Generate explicit Serde tags, renames, defaults, and unknown-field rejection.

Provider resource IDs are JSON and TypeScript strings, parsed and validated at
the provider boundary. Durations such as `timeout_seconds` remain numbers.

## Consequences

- The checked-in TypeScript is both documentation and the source of truth.
- Existing callers must send provider resource IDs as strings.
- Rust command types are generated during every build.
- SWC is a build dependency; TypeScript and Prettier are development tools.
- Snapshots review the TypeScript, generated Rust, rejected inputs, and help.
