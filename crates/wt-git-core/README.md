# wt-git-core

Shared Git service plumbing for WT's two Git frontends.

This crate owns pkt-line parsing, whole-push validation, branch write policy,
and local or SSH upstream supervision. It has no WT world state and no
standalone server configuration.

- `wt-agent-git` supplies WT world authorization and reporting.
- `wt-git-proxy` supplies standalone OpenSSH and repository configuration.
