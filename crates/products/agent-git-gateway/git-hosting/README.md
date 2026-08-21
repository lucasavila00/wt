# wt-git-hosting

Typed GitHub and GitLab operations used by the agent Git gateway.

The crate owns provider API requests, pull or merge request review operations,
CI inspection and control, and the `wt-git-hosting` command-line interface. Gateway
authentication and repository transport stay in `wt-agent-git-gateway`.
