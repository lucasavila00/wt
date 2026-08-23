# ADR 0075: Centralize holistic Git state

- Status: Proposed
- Date: 2026-08-23

## Context

WT currently stores checkout state on `codex_session_reports`, keyed by `(world_id, session_id)`:
`repository_root`, `repository_url`, `git_branch`, `git_context_checked_at_unix_ms`, and
`git_context_error`. It is therefore a side effect of Codex lifecycle reporting.

`world_git_activity` and `world_wt_tools_activity` record remote Git and provider API activity.
Each repeats `(provider_host, repository)`, so they have no shared repository identity or query.

## Decision

Make repository identity and checkout state first-class registry data.

```sql
repositories (
  id INTEGER PRIMARY KEY,
  provider_host TEXT NOT NULL,
  repository TEXT NOT NULL,
  UNIQUE (provider_host, repository)
)

codex_checkout_state (
  world_id TEXT NOT NULL REFERENCES worlds(id) ON DELETE CASCADE,
  session_id TEXT NOT NULL, cwd TEXT NOT NULL,
  repository_root TEXT, repository_url TEXT,
  repository_id INTEGER REFERENCES repositories(id), branch TEXT,
  checked_at_unix_ms BIGINT NOT NULL, error TEXT,
  pane_id TEXT NOT NULL, pane_generation BIGINT NOT NULL,
  PRIMARY KEY (world_id, session_id, cwd),
  FOREIGN KEY (world_id, session_id) REFERENCES codex_session_reports(world_id, session_id) ON DELETE CASCADE
)
```

`repositories` is the only shared repository identity. It uses the configured provider host and repository
path with one final `.git` suffix removed; repository path case is preserved. URL host matching is
case-insensitive, but aliases, redirects, and renames stay distinct until an explicit mapping policy exists.
The catalog is historical identity data, not current provider configuration or authorization, and is not deleted.

Replace both activity tables' `provider_host` and `repository` with `repository_id INTEGER NOT NULL
REFERENCES repositories(id)`. The Git gateway and `wt-tools` resolve or create the catalog row in the same
transaction as their append-only activity record. Their provenance, raw `wt-tools` JSON, and operation time stay.

Move the five checkout columns out of `codex_session_reports` into `codex_checkout_state`; `cwd` remains on
the lifecycle report. The relay is the only checkout-state writer. It transactionally accepts an update only
for the active world, session, cwd, pane, and generation; reads apply the same guard so stale sibling rows
are hidden. It never changes lifecycle state, ordering, or lifecycle receipt time.

The checkout's selected remote is explicit. `repository_id` is null for no remote, local-only, unsupported,
or ambiguous remotes; those checkout facts remain visible but unjoined. Failed checks retain the last
successful checkout facts and set `error`; a successful non-repository check clears them. Do not infer links
from legacy raw remote URLs.

Expose a typed `repository_git_state` registry query keyed by `repositories.id`. It returns active linked
checkouts and separately paginated Git-gateway and provider API histories rather than joining three
one-to-many tables. It is a derived read model, not a mutable table or authority, and keeps timestamps
separate. Index checkout by `repository_id`; index activity by `repository_id`, branch/change request, and id.

## Migration

1. Create `repositories` and `codex_checkout_state`.
2. Seed the catalog from distinct activity targets; add and backfill `repository_id` transactionally, then
   rebuild both activity tables without duplicated target columns.
3. Copy legacy checkout fields to unjoined checkout rows, then remove the five fields from reports.
4. New relay observations link a selected remote only when it maps unambiguously to a configured catalog entry.

## Consequences

Codex lifecycle, local checkout observation, Git transport, and provider API activity remain separate facts
under one repository identity. A push or PR lookup cannot claim to be the active checkout, and a local branch
switch cannot manufacture remote activity. World deletion cascades all of this state.
