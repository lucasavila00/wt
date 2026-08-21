# Avoid duplicate CI runs for pull request branches

The CI workflow runs on every `push` and every `pull_request` event. A commit
pushed to a branch with an open pull request therefore starts two equivalent
`checks` jobs for the same commit: one for the branch push and one for the pull
request.

For example, commit `c7a2fee` on `wt/check-crate-readmes` started runs
`32463374317` and `32463379335`. Both execute the same workflow and job.

Configure the workflow so an open pull request has one authoritative CI run
while preserving CI for branches that do not have a pull request. Required
checks and branch-protection behavior should continue to use the surviving
run.
