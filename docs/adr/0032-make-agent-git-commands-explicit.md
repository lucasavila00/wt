# ADR 0032: Make agent Git commands explicit

- Status: Accepted
- Date: 2026-08-14
- Amends: [ADR 0017](0017-integrate-agent-git-gateway.md) and
  [ADR 0031](0031-allow-project-wide-provider-reads.md)

## Context

`ag-git` inferred a pull or merge request and its CI from the current checkout.
Every command consequently required a prefixed branch, a commit, and an exactly
matching published head, including commands that already named a CI job.

The inferred snapshot also combined request metadata, review discussions, CI
runs, and jobs. Provider-specific concepts and failures repeatedly escaped that
model as special cases.

## Decision

Make every public provider command identify its resource type and provider ID.
The gateway grant supplies only the provider and project. The checkout supplies
no command input and is not an authorization boundary.

Use commands such as `ag-git show mr 7`, `ag-git list ci commit SHA`,
`ag-git log job 94633137834`, and `ag-git wait job 94633137834`. Model CI runs
and jobs separately. List and show operations are independent rather than one
aggregate status snapshot.

Authorize reads for the whole granted project. Before a mutation, fetch the
named resource and validate provider metadata: requests and discussions must
belong to a request from the shared branch prefix to the granted base, and CI
controls must belong to a run on the shared branch prefix.

Keep the existing `ag-git` transport binary and protocol. Older clients may
still send checkout metadata, but the gateway ignores it. Reject the old
contextual grammar instead of maintaining a second behavior mode.

## Consequences

- Commands work from any directory, branch, commit, or detached checkout.
- Output IDs can be copied directly into later commands.
- Historical and base-branch resources are readable but remain immutable.
- GitHub workflow runs and GitLab pipelines are represented explicitly.
- Existing worlds receive the new command contract when the gateway updates.
