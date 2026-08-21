# ADR 0041: Publish a standalone Git proxy

- Status: Accepted
- Date: 2026-08-19

Publish `wt-git-proxy` as a separate binary and keep shared Git transport and
write policy in `wt-git-smart-protocol`. WT world behavior stays in the agent
tool gateway; standalone OpenSSH configuration stays in the proxy.

OpenSSH runs the proxy as a forced command. Each Git host uses its own SSH
credential and pinned host key. A required branch prefix and optional exact
branch list control writes; tags and other refs are denied. The standalone
proxy has no WT world, grant, token, or registry behavior.
