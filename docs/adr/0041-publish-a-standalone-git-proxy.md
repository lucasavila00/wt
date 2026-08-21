# ADR 0041: Publish a standalone Git proxy from WT

- Status: Accepted
- Date: 2026-08-18
- Related: [ADR 0017](0017-integrate-agent-git-gateway.md)

## Context

WT's Git gateway is also useful for devcontainer hosts and cloud VMs that do
not run WT. Maintaining a second implementation would duplicate the risky Git
protocol and write-policy code.

## Decision

Publish `wt-git-proxy` as a separate binary from this repository. Put the
shared Git transport and policy in `wt-git-smart-protocol`; keep WT world
behavior in `wt-agent-git-gateway` and standalone OpenSSH configuration in
`wt-git-proxy`.

OpenSSH runs the proxy as a forced command for each connection. Client access
is just one managed `authorized_keys` file: the TUI can add an existing public
key, generate a ready-to-copy client key bundle, list keys, and remove keys.
There are no grants, tokens, expiries, control socket, or background service.

The server has one write policy: a required branch prefix and an optional list
of exact branches. The list may be empty.
Tags and other refs are denied, and one denied ref rejects the whole push.

Each configured Git host uses its own SSH credential and pinned host key. The
client puts that host in the repository path, such as
`github.com/lucasavila00/wt.git`. The proxy has no WT world or registry
features. A real Git and two-hop OpenSSH test lives in `wt-end-to-end-tests`.

## Consequences

The standalone tool stays small and uses ordinary OpenSSH administration, while
both products exercise the same Git security boundary.
