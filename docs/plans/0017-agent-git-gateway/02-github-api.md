# Milestone 2: GitHub API support

Keep the transport and CLI from milestone 1. Add GitHub authentication and the
provider implementation inside the gateway. Validate the configured API
identity and repository permissions before creating a GitHub world.

Implement the operations in this order:

1. Find or open the current branch's pull request, including draft requests.
2. Show and edit the request, mark it draft or ready, and close or reopen it.
3. Read, reply to, resolve, and reopen review threads.
4. Manage comments, reviewers, assignees, and labels.
5. Show checks and logs, control eligible jobs, and wait for review or CI state
   to change.

Every operation stays inside the world's project, base, prefix, and current
commit. The gateway refuses merging, approval, base changes, review dismissal,
and work outside that scope. No provider credential crosses into `wt-server`, a
guest, or a devcontainer.

Tests use local HTTP fixtures. They need no GitHub credential and never call the
real API. Before release, a human runs the documented workflow against a
dedicated GitHub test repository. QA credentials stay in the installed gateway
and never enter the repository or test harness.

This milestone is complete when every documented command works for GitHub and
GitLab commands still report that their API implementation is unavailable.
