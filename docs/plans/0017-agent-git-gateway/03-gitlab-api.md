# Milestone 3: GitLab API support

Implement the same provider contract for GitLab merge requests, discussions,
pipelines, and jobs. Validate the configured API identity and repository
permissions before creating a world for a GitLab repository. Keep command names
and output provider-neutral except for native request and job identifiers.

Reuse the GitHub provider tests as the contract and add local HTTP fixtures for
GitLab wire behavior. Tests need no GitLab credential and never call the real
API.

Before release, a human runs the documented workflow against a dedicated
GitLab test project. QA credentials stay in the installed gateway and never
enter the repository or test harness.

This milestone is complete when the same CLI workflow works for GitHub and
GitLab, with provider differences contained inside the gateway.
