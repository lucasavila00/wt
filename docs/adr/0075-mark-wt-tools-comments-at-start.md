# ADR 0075: Mark wt-tools comments at their start

- Status: Accepted; Date: 2026-08-23

## Decision

Every pull-request or merge-request comment created by `wt-tools` begins with
this exact marker, with no preceding bytes:

```text
<!-- wt-tools-comment -->

```

The marker precedes the caller-supplied body and is added by the gateway for
both general comments (`comment_mr`) and review-thread replies
(`reply_thread`). It is provider-neutral and intentionally remains in the
stored comment body so GitHub and GitLab return the same ownership signal.

`edit_comment` and `delete_comment` accept only a general comment whose body
starts with this exact marker. They retain the existing repository and MR
membership checks, writable `wt/*` MR scope, merged-MR confirmation, and
provider authorization checks. An edit writes exactly one marker followed by
the replacement body; it does not allow the caller to remove, move, or
duplicate the marker.

Existing comments that have only the earlier WT attribution footer, or no
marker, are not editable or deletable through `wt-tools`. The gateway does not
try to migrate them because doing so would require the very mutation this rule
is intended to guard.

## Consequences

The gateway has a deterministic, provider-independent provenance boundary for
comment mutation. Markers are visible in raw Markdown but not rendered by
GitHub or GitLab, so they do not add reader-facing text. Callers can still list
and show every general comment, while mutation remains limited to comments
created under this policy.
