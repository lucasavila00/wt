# ADR 0007: Use Diesel for the registry

- Status: Proposed
- Date: 2026-07-17

## Context

`wt-server` uses `rusqlite` directly. Queries repeat column names and decode
rows by column position. Schema setup is hand-written in `Store::open`.

We want simple database code and automatic migrations. The store is
synchronous and uses SQLite.

## Decision

Replace `rusqlite` with Diesel and its SQLite backend.

- Keep the current `Store` API.
- Use Diesel models and query builders for normal database work.
- Keep raw SQL only when it is clearer, such as SQLite connection settings.
- Keep SQLite bundled in the binary.
- Store migrations in `wt-server` and embed them in the binary.
- Run pending migrations in `Store::open`. Fail startup if they fail.
- Use explicit SQL migrations. Do not generate schema changes at runtime.

The first migration creates the whole schema. We will run `make clear` during
the change. We will not migrate or detect the old database format.

The Diesel CLI may help developers create migrations. It is not needed on the
server.

## Consequences

- Queries and row types are checked at compile time.
- New migrations run automatically when `wt-server` starts.
- Schema changes stay explicit and reviewable.
- Diesel adds build time and dependencies.
- Old development registry data is deleted.

## Alternatives

### Keep `rusqlite`

A migration library could handle upgrades, but database code would still use
manual SQL and row positions.

### Use SQLx

SQLx still uses SQL strings and is async-first.

### Use SeaORM

SeaORM is async and has more machinery than this store needs.
