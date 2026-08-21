# ADR 0049: Generate TypeScript command types

- Status: Proposed; Date: 2026-08-21

## Context

`wt-tools help` manually duplicates the Serde `WtToolsCommand` contract as a
TypeScript definition. The two can drift whenever a variant or field changes.

## Decision

Use
[`typescript-type-def`](https://crates.io/crates/typescript-type-def) to derive
the TypeScript definition from `WtToolsCommand` and its referenced types. It
uses Serde's tags, renaming, defaults, and optional fields as the wire contract.

Render the declaration into the help text from Rust. Cover it with the existing
complete help snapshot so both contracts change together in review.

Represent provider resource IDs as JSON and TypeScript strings. Parse and
validate them in Rust at the provider boundary. This avoids JavaScript integer
precision limits and a `bigint` representation that JSON cannot encode.
Durations such as `timeout_seconds` remain numbers.

## Consequences

- Rust command types become the source of truth for the TypeScript contract.
- Existing callers must send provider resource IDs as strings.
- Protocol types gain a TypeScript-generation derive and dependency.
- Unsupported Serde behavior requires a snapshot-reviewed explicit mapping.
