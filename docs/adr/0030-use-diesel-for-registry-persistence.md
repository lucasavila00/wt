# ADR 0030: Use Diesel for the registry

- Status: Accepted
- Date: 2026-07-17

## Context

The synchronous SQLite store needs compile-time checked queries, typed row
decoding, and explicit migrations without an asynchronous runtime.

## Decision

Replace `rusqlite` with Diesel and its SQLite backend.

- Keep the current `Store` API.
- Use Diesel models and query builders for normal database work.
- Keep raw SQL only when it is clearer, such as SQLite connection settings.
- Keep SQLite bundled in the binary.
- Store migrations and generated schema in `wt-workload-registry` and embed them.
- Run pending migrations when the registry opens. Fail startup if they fail.
- Use explicit SQL migrations. Do not generate schema changes at runtime.

The first migration creates the whole schema. Run Diesel CLI commands from
`crates/shared/workload-registry`. Its config regenerates `src/schema.rs`. The
CLI is not needed on the server.

## Consequences

- Queries and row types are checked at compile time.
- New migrations run automatically when a registry owner starts.
- Schema changes stay explicit and reviewable.
- Diesel adds build time and dependencies.

## Alternatives

### Keep `rusqlite`

A migration library could handle upgrades, but database code would still use
manual SQL and row positions.

### Use SQLx

SQLx still uses SQL strings and is async-first.

### Use SeaORM

SeaORM is async and has more machinery than this store needs.
