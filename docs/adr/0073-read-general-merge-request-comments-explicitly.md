# ADR 0073: Read general merge-request comments explicitly

- Status: Accepted; Date: 2026-08-23

## Decision

Expose general merge-request comments through two `wt-tools` operations:

```text
{ action: "list_comments"; mr: string }
{ action: "show_comment"; mr: string; comment: string }
```

Each comment contains its provider-native numeric handle, author, body, URL,
and provider `created_at` and `updated_at` timestamp strings. Missing authors
are reported as `unknown`. GitHub handles are issue-comment database IDs, so a
handle copied from a `#issuecomment-ID` URL works directly. GitLab handles are
merge-request note IDs; their URLs use the provider's `#note_ID` anchor.

General comments remain separate from `show_mr` and `list_threads`. On GitHub
they are pull-request issue comments, not review comments. On GitLab they are
non-system, non-resolvable merge-request notes; system and resolvable review
notes are excluded. Discussion replies remain in `list_threads`.

`list_comments` reads every provider page in chronological order.
`show_comment` uses a direct lookup and verifies the comment belongs to the
explicit repository and merge request. It therefore remains usable for an old
linked comment without first listing the full history, and it cannot use a
valid provider comment ID to cross the requested merge-request boundary.

## Consequences

Agents can inspect general feedback and resolve GitHub issue-comment links
without bypassing the repository-owned gateway. Callers choose explicitly
between conversation comments and actionable review threads. The provider
timestamp format is preserved rather than normalized, and comment lists can
require multiple provider requests.
