# ADR 0018: Publish agent work through leased assignments

- Status: Proposed
- Date: 2026-08-09

## Context

ADR 0017 gives an agent full control of its private Forgejo fork. It must not
turn every branch in that fork into something the agent can publish externally.

Branch protection helps, but it is not a narrow credential boundary.

## Decision

An explicit WT assignment is the permission to publish. It binds one world,
repository, source ref, base ref, external staging ref, and pull or merge
request. It also records the expected external commit as a lease and whether
history rewrites are allowed.

There are no wildcard refs. Creating a Forgejo branch grants no external
authority. Only an authenticated WT client can create or change an assignment.

External heads live in a WT-owned staging fork, not the canonical repository.
The adapter must preserve the forge's untrusted-fork CI boundary.

The WT publisher reads its destination from the assignment, never from agent
input. It copies the assigned commit, updates exactly the assigned staging ref
with its lease, and creates or updates the assigned PR or MR. An unexpected
external change is a conflict, not an overwrite.

The first capability may push commits and edit the assigned review's title and
description. It cannot approve, merge, close, publish tags, change reviewers or
settings, delete external branches, or touch another review.

GitHub App or GitLab service credentials remain on the WT server. They are
short-lived or expiring and limited to the required repositories and
operations. The publisher never uses mirror pushes, wildcard refspecs, or
unleased force pushes.

Retries are durable and idempotent; a timeout is not success. Deleting a world
discards unpublished work but keeps already published branches and reviews.

The first milestone uses normal `wt new` and is complete when an agent can open
and update one real GitHub or GitLab review. ADRs 0019 and 0020 are later work.

## Verification

- Unassigned refs never appear on the external forge.
- An assignment can update only its exact leased ref and review.
- External changes cause conflicts.
- The agent cannot obtain external credentials, approve, merge, or publish
  another ref.

## Consequences

The agent gets a normal Git workflow while external authority stays behind a
small publisher. WT must coordinate durable state across Forgejo and the
external forge.

The server still holds broader credentials than a world. Splitting staging and
review authority reduces, but does not remove, that host-level risk.

## Alternatives

Publishing every proxy branch makes branch creation a permission. Direct
canonical pushes weaken isolation. A general forge API proxy is harder to audit
than a few fixed operations.

## References

- [GitHub App installation tokens](https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/authenticating-as-a-github-app-installation)
- [GitLab project access tokens](https://docs.gitlab.com/user/project/settings/project_access_tokens/)
