# wt-tools

Typed GitHub and GitLab operations used by the agent tool gateway.

The crate owns provider API requests, pull or merge request review operations,
CI inspection and control, and the `wt-tools` command-line interface. Gateway
authentication and repository transport stay in `wt-agent-tool-gateway`.
