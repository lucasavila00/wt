# Stage 3: GitLab API support

Implement the same gateway contract for GitLab. Command names and output stay
provider-neutral; only native merge request, pipeline, and job identifiers are
shown.

## API

Use GitLab GraphQL for the stable merge request and review operations it covers.
Commit GitLab's machine-readable
[GraphQL schema](https://docs.gitlab.com/api/graphql/reference/) without
deprecated fields and commit each query. Generate request and response types at
compile time. Do not use experimental fields.

Use REST v4 for discussions, pipelines, jobs, traces, and controls when the
GraphQL schema is missing an operation or gives it weaker compatibility. Keep
these calls as small typed request and response structs, checked against
GitLab's [OpenAPI contract](https://docs.gitlab.com/api/openapi/). Do not
generate a full GitLab client.

The provider maps both APIs into the same provider-neutral types used by
GitHub. A GitLab-specific exception belongs in the provider, not the CLI.

## Order

1. Authenticate the API identity and read the project, base branch, and
   effective permission before creating a world.
2. Find, open, show, edit, draft, ready, close, and reopen the branch's merge
   request.
3. Read, reply to, resolve, and reopen discussions. Add normal comments.
4. Show pipelines, jobs, and traces; control eligible jobs; and wait for review
   or CI state to change.

Every request is rebuilt from the grant's project, base, prefix, and current
commit. Provider IDs returned by an earlier call are never trusted on their
own. The gateway keeps the same restrictions as GitHub.

## Automated tests

Tests never use a GitLab credential or contact GitLab.

- Compile every GraphQL query against the vendored schema.
- Reuse the provider-neutral command and output tests used by GitHub.
- Run the GitLab client against local HTTP fixtures. Assert the method, path,
  authentication, and relevant request body, then parse representative success
  and failure responses. Never return partial request, review, or CI state;
  paginate it or fail clearly when the provider reports another page.
- Reuse the provider-neutral output snapshots.
- Add a small set of KVM cases that run `ag-git` through the real CLI, relay,
  and gateway into the local HTTP fixture. They cover the important wiring
  paths without repeating the provider test suite.

Fixtures contain invented projects, users, commits, and request IDs. They are
written by hand from the public contract, not recorded from a real account.

## Human QA

Before release, a maintainer runs the same command workflow against a dedicated
GitLab test project. This is the only test of a real GitLab instance's
permissions, discussion behavior, and CI behavior. The credential stays in the
installed gateway; QA is not an automated test or CI job.

Stage 3 is complete when the same CLI workflow works for GitHub and GitLab,
with provider differences contained inside the gateway.
