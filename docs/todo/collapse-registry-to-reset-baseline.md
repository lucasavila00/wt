# Collapse the registry to one reset-era schema baseline

WT deploys incompatible registry changes with `make clear`, so the migration
chain should not retain historical evolution.

- Fold `agent_tool_reports`, `worlds.created_at`, and world activity tables and
  indexes into `00000000000000_create_registry`.
- Delete `00000000000001_create_agent_tool_reports`,
  `20260822190000_add_world_creation_time`, and
  `20260823020000_add_world_activity_history`, including the creation-time
  backfill.
- Remove migration-chain tests or code made unnecessary by the single baseline.

This requires `make clear` before deployment; do not add a compatibility path
or data migration.
