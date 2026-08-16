# ADR 0035: Store agent Git reports in the world registry

- Status: Accepted
- Date: 2026-08-16
- Extends: [ADR 0017](0017-integrate-agent-git-gateway.md)
- Extends: [ADR 0034](0034-use-json-for-agent-git-commands.md)

## Context

Agents can encounter defects, confusing behavior, and missing capabilities in
the `ag-git` tool while working inside a world. They cannot contact the
developer directly, and provider comments are the wrong destination for
feedback about the gateway itself.

The agent Git gateway already authenticates every `ag-git` request with a grant
bound to one world. The WT server registry is the durable source of world
identity and is available to both the gateway and server processes.

Reports must be discoverable without changing a healthy world's lifecycle
status into an error. This is not a general-purpose bug reporting channel:
every accepted category explicitly concerns `ag-git`.

## Decision

Add four strict JSON actions to `ag-git`:

- `report_ag_git_bug`
- `report_ag_git_issue`
- `suggest_ag_git_improvement`
- `request_ag_git_feature`

Each action accepts one non-empty `description` field. These actions are
handled by the gateway before Git provider selection and never require or
contact a Git provider API. The gateway derives the reporting world from the
authenticated grant; callers cannot supply or override a world identifier.

Persist every report in an `agent_git_reports` table in the shared SQLite
registry. Each row records its world, category, and description. Reports are
removed when their world is removed.

Expose owner-scoped `list_agent_git_reports` and `clear_agent_git_reports`
server operations. The human CLI presents these as `wt reports` and
`wt clear-reports`. `wt ls` shows an `ag-git` report count and directs the user
to `wt reports`, but does not alter the world's lifecycle status or
`last_error`.

## Consequences

- Agents have a credential-free channel for durable `ag-git` feedback.
- A report is attributable only to the world whose grant authenticated it.
- Bugs, general issues, improvements, and feature requests remain distinct
  without requiring separate tables or workflows.
- The interface does not imply that unrelated WT or project reports belong in
  this channel.
- Reports from another server user are neither listed nor cleared.
- `wt ls` makes pending feedback visible while preserving runtime status.
- Clearing reports removes every visible `ag-git` report in each successfully
  contacted context.

