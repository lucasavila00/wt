# ADR 0002: Publish agent work through leased assignments

- Status: Proposed
- Date: 2026-08-09

## Decision

An assignment is the permission to publish. It binds:

- one authenticated, project-scoped publisher;
- one workspace source ref and target base;
- one external staging ref and pull or merge request;
- the expected external commit used as a lease; and
- whether history rewrites are allowed.

Workspace Git credentials cannot use the assignment API. Only a trusted caller
with the project's publish role can create, change, or revoke an assignment.
There are no wildcard refs.

Once assigned, later pushes to the source ref update the same review. Every
other workspace ref stays private.

For each update, the gateway atomically resolves the source ref and pins its
commit under an immutable internal ref. Validation and transfer use that pinned
commit.

External heads live in a gateway-owned staging fork. The publisher updates only
the assigned staging ref with its lease and creates or updates only the
gateway-created review. Any project, ref, base, head, review, or lease mismatch
is a conflict.

The publisher may push commits and edit the assigned review's title and
description. It cannot approve, merge, close, publish tags, change reviewers or
settings, delete branches, or touch another review.

Mirror reads, staging writes, and review metadata use separate external
credentials. None can write canonical content or manage merges, secrets, hooks,
workflows, or repository access.

External review CI is untrusted. It receives no protected secrets, write token,
privileged runner, or trusted-branch treatment.

Revoking a workspace closes its assignments, rejects queued work, and cancels
or drains pinned publication before acknowledging the fence. Published branches
and reviews remain.

## Verification

- Unassigned refs never appear on the external forge.
- Branch rewrites after pinning do not change the published commit.
- An assignment updates only its leased staging ref and review.
- Agents cannot obtain external credentials, approve, merge, or publish another
  ref.
