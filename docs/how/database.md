# Database

`wt-server` uses SQLite through Diesel.

- Database: `~/.local/state/wt/instances.db`
- Queries and models: `crates/wt-server/src/store.rs`
- Generated schema: `crates/wt-server/src/schema.rs`
- Migrations: `crates/wt-server/migrations/`
- Diesel config: `crates/wt-server/diesel.toml`

SQLite is bundled. The server does not need Diesel CLI.

## Startup

`Store::open` opens SQLite and runs pending migrations. Migrations are embedded
in the binary. If a migration fails, startup fails.

## Change the schema

Run commands from the server crate:

```sh
cd crates/wt-server
diesel migration generate MIGRATION_NAME
```

Edit `up.sql` and `down.sql`. Then run the migration on a development database:

```sh
diesel migration run --database-url /tmp/wt-development.db
```

This also regenerates `src/schema.rs`.

- Commit the migration and `schema.rs` together.
- Do not edit `schema.rs` by hand.
- Do not edit merged migrations. Add a new migration.

Run `make clear` once when moving from the old `rusqlite` database. Later
schema changes migrate automatically.

See [ADR 0007](../adr/0007-use-diesel-for-registry-persistence.md).
