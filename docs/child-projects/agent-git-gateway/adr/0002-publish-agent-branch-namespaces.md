# ADR 0002: Publish agent branch namespaces

- Status: Proposed
- Date: 2026-08-09

## Decision

Workspace creation may include a publication grant containing:

- one external project and target base;
- one branch prefix reserved for that workspace;
- whether history rewrites are allowed; and
- service limits for reviews and updates.

The prefix is the workspace name followed by `/`. It is immutable and not
configurable. A workspace named `df1` always gets `df1/`. Workspace creation
fails if that prefix cannot be reserved for the project.

Every branch under the prefix is published automatically:

```text
df1/fix-a       -> one external review
df1/refactor-b  -> another external review
scratch         -> private to the workspace
```

The gateway turns each matching branch into an exact assignment. It binds the
source ref, target base, external staging ref, pull or merge request, expected
external commit, and rewrite policy.

For each update, the gateway atomically resolves the source ref and pins its
commit under an immutable internal ref. It updates only the assignment's
staging ref with its lease and creates or updates only its gateway-created
review. Any project, ref, base, head, review, or lease mismatch is a conflict.

The workspace Git credential cannot use the control plane or change its grant.
Only a trusted project publisher can create, change, or revoke a grant.

The publisher may push commits and edit the review title and description. It
cannot approve, merge, close, publish tags, change reviewers or settings,
delete external branches, or touch another review.

Mirror reads, staging writes, and review metadata use separate external
credentials. External review CI is untrusted and receives no protected secrets,
write token, privileged runner, or trusted-branch treatment.

Revocation disables the grant, rejects queued updates, and cancels or drains
pinned publication before acknowledging the fence. Published branches and
reviews remain.

## Verification

- Matching branches create distinct reviews without another user action.
- Non-matching branches never appear on the external forge.
- Branch rewrites after pinning do not change the published commit.
- Each assignment updates only its leased staging ref and review.
- Agents cannot change their grant, obtain external credentials, approve, or
  merge.
