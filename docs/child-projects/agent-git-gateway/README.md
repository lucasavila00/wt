# Agent Git gateway

The agent Git gateway gives each agent its own private Forgejo fork and
publishes its branches to GitHub or GitLab under a fixed namespace.

The gateway is a standalone service. It knows about projects, Forgejo mirrors,
private forks, public keys, and branch mappings. It does not know about WT, VMs,
or devcontainers. WT is one client of the gateway.

## Flow

1. A client supplies a project, namespace, base branch, and public key. WT uses
   the world name as the namespace.
2. The gateway creates or reuses the private Forgejo fork for that project and
   namespace, authorizes the public key, and returns the Git remotes.
3. The agent uses the private fork as `origin` and the read-only project mirror
   as `upstream`.
4. A branch such as `fix-login` is published to the external project as
   `<namespace>/fix-login`.
5. Later pushes, force-pushes, and deletions update the same external branch.

The gateway never receives the private key and does not remove private forks or
authorized public keys.

## Decision

- [ADR 0001: Provide private Forgejo forks and publish agent branches](./adr/0001-provide-private-forgejo-forks-and-publish-agent-branches.md)
