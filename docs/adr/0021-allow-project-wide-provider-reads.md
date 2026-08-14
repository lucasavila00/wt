# ADR 0021: Allow project-wide provider reads

- Status: Accepted
- Date: 2026-08-14
- Amends: [ADR 0017](0017-integrate-agent-git-gateway.md)'s CI scope

## Context

Every WT world receives a gateway grant for one project, base branch, and
shared branch namespace. `ag-git` originally applied the current checkout's
commit boundary to both CI controls and CI log reads.

That prevents an agent from monitoring a merged change on the base branch or
inspecting a historical job, even when the user supplies the provider's job ID.
Reading that job does not expand the world's project access or mutate provider
state.

## Decision

Authorize provider reads across the grant's project when an `ag-git` command
directly identifies the provider object. Keep discovery centered on the current
branch and commit so ordinary status output remains relevant.

As the first application of this policy, `ag-git log JOB` accepts any numeric
GitHub Actions or GitLab CI job ID in the granted project. It reads through a
project-qualified provider endpoint, so a job ID from another project is still
rejected by GitHub or GitLab. The gateway does not accept a project override.

Keep mutations scoped more narrowly. `ag-git retry JOB` and `ag-git cancel JOB`
still require a job from the current commit, and pull or merge request and
review mutations retain ADR 0017's branch and request boundaries. `ag-git ci`
continues to discover jobs for the current commit.

Future read-only commands may use the same project-wide rule. Each must use a
provider endpoint qualified by the granted project and must not turn a read
handle into authority for a later mutation.

## Consequences

- Agents can monitor merged, base-branch, and historical CI jobs with a job ID.
- A world still cannot read another project's jobs through the gateway.
- Read and mutation handles intentionally have different authorization rules.
- Job URLs are not a new input format; callers pass the numeric job ID shown in
  the URL.
