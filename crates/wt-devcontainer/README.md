# wt-devcontainer

Devcontainer world lifecycle.

It creates the guest through `wt-provider`, bootstraps Ubuntu, checks out the
repository, starts the devcontainer, installs app SSH helpers, and verifies
readiness. Start restores the existing containers and SSH access.

Machine backends stay in provider crates. Git transport stays in
`wt-agent-git-gateway`.

Contract: [Devcontainer worlds](../../docs/worlds/devcontainer.md).
