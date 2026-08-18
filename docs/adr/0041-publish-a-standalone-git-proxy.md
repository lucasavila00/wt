# ADR 0041: Publish a standalone Git proxy from WT

- Status: Proposed
- Date: 2026-08-18
- Related: [ADR 0017](0017-integrate-agent-git-gateway.md)

## Context

An SSH Git proxy is useful for disposable or constrained environments outside
WT. It solves many of the same Git transport and authorization problems as
WT's agent Git gateway. A separate implementation would duplicate
security-sensitive code and tests.

The proxy is still useful on its own. It should not require WT worlds,
`wt-server`, libvirt, or the WT registry.

## Decision

Publish `wt-git-proxy` as a separate binary from the WT repository. Start with
it as another `wt-agent-git` binary target so it can share the gateway's Git
policy, transport code, and tests.

OpenSSH runs the binary as the forced command for a dedicated account with no
shell or forwarding. A background service uses a separate SSH identity and
pinned host keys to reach configured upstream repositories.

An operator TUI configures public-to-upstream repository mappings and
credentials, then creates and revokes client grants. Each grant has an expiry,
a required fully qualified branch prefix, and a list of exact allowed branches,
which may be empty. Reads can access every ref in the configured repositories.
A write is allowed when its branch exactly matches the list or is under the
prefix. The rule applies equally to creates, updates, force pushes, and deletes.
Tags and other refs are denied, and one denied ref rejects the whole push before
it reaches upstream.

The TUI accepts a client public key or generates a dedicated Ed25519 key pair.
It installs the public key as a restricted forced-command entry tied to the
grant. For a generated key, it returns the private key once with ready-to-copy
SSH config and pinned host keys; the server keeps only the public key and grant.
The client needs only Git and OpenSSH. The TUI may exit while the proxy service
keeps running.

A grant cannot select an unconfigured repository or widen its write policy.
The binary does not expose WT world or provider-API features.

`wt-git-proxy` is released by WT but operated separately. `wt-server-setup`
does not install or manage it, and WT does not depend on it.

Test shared behavior once. Keep the standalone two-hop, real-Git and OpenSSH
tests in `wt-integration-tests`, including malformed pushes and secret
redaction.

## Consequences

- WT and the standalone proxy use the same Git security boundary and tests.
- The proxy remains usable without installing or running WT.
- WT releases include one more independently operated binary.
