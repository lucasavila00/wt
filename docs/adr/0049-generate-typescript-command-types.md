# ADR 0049: Generate TypeScript command types

- Status: Proposed; Date: 2026-08-21

## Context

`wt-tools help` manually duplicates the Serde `CliCommand` contract as a
TypeScript definition. The two can drift whenever a variant or field changes.

## Decision

Use [`ts-rs`](https://crates.io/crates/ts-rs) to derive the TypeScript
definition from `CliCommand` and its referenced types. `ts-rs` supports Serde's
internally tagged enums, renaming attributes, defaults, and optional fields.

Render the declaration into the help text from Rust. Cover it with the existing
complete help snapshot so both contracts change together in review.

Represent provider resource IDs as JSON and TypeScript strings. Parse and
validate them in Rust at the provider boundary. This avoids JavaScript integer
precision limits and avoids `ts-rs`'s default `bigint` representation. Durations
such as `timeout_seconds` remain numbers. Use explicit overrides only when the
serialization behavior cannot be inferred from Serde attributes.

## Consequences

- Rust command types become the source of truth for the TypeScript contract.
- Existing callers must send provider resource IDs as strings.
- Protocol types gain a TypeScript-generation derive and dependency.
- Unsupported Serde behavior requires an explicit mapping and snapshot review.
