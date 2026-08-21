# ADR 0047: Make agent Git output JSON-only

- Status: Accepted
- Date: 2026-08-21
- Amends: [ADR 0034](0034-use-json-for-agent-git-commands.md)'s output decision

## Context

`ag-git` is an agent interface. Provider operations already require one typed
JSON command object, but their results are rendered as human-readable text.
Agents must parse positional lines to recover resource IDs, states, and URLs.

Adding an optional JSON mode would create two output contracts. Every new
result field and error would need both a text representation and a JSON
representation even though interactive use is not ag-git's purpose.

## Decision

Return JSON for every provider operation. Do not add an output mode or retain
the current text result format.

Use a versioned, tagged envelope for successful results and errors. Preserve
typed distinctions between merge requests, review threads, CI runs, CI jobs,
logs, and confirmations. Represent missing values as JSON `null`, not display
placeholders such as `unknown` or `unavailable`.

Keep `ag-git help`, `--help`, and `-h` as plain text. They document the command
schema for agents and are not provider-operation results.

## Consequences

- Agents consume one structured input and output contract.
- Result parsing no longer depends on display formatting.
- Existing callers that parse text output must switch to JSON.
- Human inspection remains possible through formatted JSON and provider URLs.

## Alternatives

An optional `--json` mode is rejected because it preserves two contracts
without a real interactive-user need. Keeping text-only output is rejected
because transcript readability does not justify untyped agent results.
