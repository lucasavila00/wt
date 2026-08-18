# ADR 0041: Publish a standalone Git proxy from WT

- Status: Proposed
- Date: 2026-08-18
- Related: [ADR 0017](0017-integrate-agent-git-gateway.md)

## Context

The separate `gitproxy` prototype is a useful tool, but it solves many of the
same Git transport and authorization problems as WT's agent Git gateway.
Keeping two implementations would duplicate security-sensitive code and tests.

The proxy is still useful on its own. It should not require WT worlds,
`wt-server`, libvirt, or the WT registry.

## Decision

Publish `wt-git-proxy` as a separate binary from the WT repository. Start with
it as another `wt-agent-git` binary target so it can share the gateway's Git
policy, transport code, and tests.

The binary keeps the standalone `gitproxy` model: OpenSSH runs it as a forced
command, and signed expiring capabilities limit writes to a configured branch
prefix. It does not expose WT world or provider-API features.

`wt-git-proxy` is released by WT but operated separately. `wt-server-setup`
does not install or manage it, and WT does not depend on it.

Test shared behavior once. Keep the standalone real-Git and OpenSSH tests in
`wt-integration-tests`. Once the useful standalone behavior has moved, retire
the separate `gitproxy` repository.

## Consequences

- WT and the standalone proxy use the same Git security boundary and tests.
- The proxy remains usable without installing or running WT.
- WT releases include one more independently operated binary.
