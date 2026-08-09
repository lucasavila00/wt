# Stage 2: GitHub API support

Keep the transport and CLI from stage 1. Add the GitHub provider inside the
gateway.

## API

Use GitHub GraphQL for pull requests, reviews, threads, comments, reviewers,
assignees, labels, and check status. Commit GitHub's
[public schema](https://docs.github.com/en/graphql/overview/public-schema) and
the queries to the repository. Generate request and response types at compile
time. Builds never download a schema or introspect GitHub.

Use the versioned REST API only where GraphQL is not enough, notably Actions
logs and run or job controls. Keep these calls as small typed request and
response structs; do not generate a full GitHub client.

The provider maps both APIs into provider-neutral gateway types. No GitHub type
crosses into command handling or user-visible output.

## Order

1. Authenticate the API identity and read the repository, base branch, and
   effective permission before creating a world.
2. Find, open, show, edit, draft, ready, close, and reopen the branch's pull
   request.
3. Read, reply to, resolve, and reopen review threads. Add normal comments.
4. Manage reviewers, assignees, and labels.
5. Show checks and logs, control eligible Actions jobs, and wait for review or
   CI state to change.

Every request is rebuilt from the grant's project, base, prefix, and current
commit. Provider IDs returned by an earlier call are never trusted on their
own. The gateway still refuses merge, approval, base changes, review dismissal,
and work outside the grant.

## Automated tests

Tests never use a GitHub credential or contact GitHub.

- Compile every GraphQL query against the vendored schema.
- Test provider-neutral command behavior with an in-memory provider.
- Run the GitHub client against local HTTP fixtures. Assert the complete
  method, path, query, headers, and body, then parse representative success,
  pagination, GraphQL error, REST error, and rate-limit responses.
- Snapshot complete `ag-git` output from provider-neutral results.
- Add a small set of KVM cases that run `ag-git` through the real CLI, relay,
  and gateway into the local HTTP fixture. They cover the important wiring
  paths without repeating the provider test suite.

Fixtures contain invented repositories, users, commits, and request IDs. They
are written by hand from the public contract, not recorded from a real account.

## Human QA

Before release, a maintainer runs the documented command workflow against a
dedicated GitHub test repository. This is the only test of GitHub's real
permissions, review behavior, and Actions behavior. The credential stays in the
installed gateway; QA is not an automated test or CI job.

Stage 2 is complete when every documented command works for GitHub and GitLab
commands still report that their provider is unavailable.
