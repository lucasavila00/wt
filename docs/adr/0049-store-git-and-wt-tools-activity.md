# ADR 0049: Store Git and wt-tools activity

- Status: Accepted; Date: 2026-08-23

## Decision

Store Git and `wt-tools` history in separate SQLite tables. Their data and
query paths are different. `world_wt_tools_activity` contains only targeted
Git-hosting commands; feedback remains `agent_tool_reports`.

```sql
world_git_activity (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  world_id TEXT NOT NULL REFERENCES worlds(id) ON DELETE CASCADE,
  recorded_at_unix_ms BIGINT NOT NULL,
  kind TEXT NOT NULL,
  provider_host TEXT NOT NULL,
  repository TEXT NOT NULL,
  git_service TEXT,
  branch TEXT,
  previous_oid TEXT,
  new_oid TEXT
)

world_wt_tools_activity (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  world_id TEXT NOT NULL REFERENCES worlds(id) ON DELETE CASCADE,
  recorded_at_unix_ms BIGINT NOT NULL,
  provider_host TEXT NOT NULL,
  repository TEXT NOT NULL,
  action TEXT NOT NULL,
  branch TEXT,
  change_request TEXT,
  request_json TEXT NOT NULL CHECK(json_valid(request_json)),
  response_json TEXT NOT NULL CHECK(json_valid(response_json))
)
```

- Git rows record service requests and successful branch updates. A push writes
  one row per updated branch with its old and new object IDs.
- `wt-tools` rows keep the exact JSON request and response. Its raw JSON is the
  forward-compatible record for provider-specific and future command fields.
- Git intentionally stores only the listed structured fields. Discard unknown
  Git protocol data; add a column and index only when a feature needs it.
- `branch` is the updated Git branch or the `wt-tools` head branch. It is never
  the merge-request base branch.
- `change_request` is populated from the typed command or response when known.
- New `wt-tools` fields stay in JSON. Add a column only when a new query needs
  an indexed value.

SQLite is appropriate because WT already owns its lifecycle, provides atomic
writes and cascade deletion, and indexes branch and change-request lookup.
JSONL would require a separate index for the same queries.

Normalize Git and `wt-tools` targets through one function: lowercase configured
host and repository path with one final `.git` suffix removed. Aliases,
redirects, case variants, and renames remain distinct.

Indexes:

```sql
world_git_activity (world_id, id DESC)
world_git_activity (provider_host, repository, branch, id DESC)
world_wt_tools_activity (world_id, id DESC)
world_wt_tools_activity (provider_host, repository, branch, id DESC)
world_wt_tools_activity (provider_host, repository, change_request, id DESC)
```

The gateway takes `world_id` only from the authenticated grant.

- Record Git service requests before forwarding them.
- Record Git branch updates after receive-pack confirms the ref succeeded.
- Record a `wt-tools` row after it has a JSON response, with the request and
  response written in the same transaction.
- A failed pre-forward Git service write rejects the request. A failed branch
  update or `wt-tools` write preserves the completed external result and logs
  the missing history.

Add owner-scoped exact queries:

- `git_world { world_id, before_id? }`
- `git_branch { provider_host, repository, branch, before_id? }`
- `wt_tools_world { world_id, before_id? }`
- `wt_tools_branch { provider_host, repository, branch, before_id? }`
- `change_request { provider_host, repository, handle, before_id? }`

Return newest-first, at most 200 rows. Cursors are exclusive. Delete rows with
their world. Retention is a separate decision.
