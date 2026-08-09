# ADR 0017: Isolate agent Git work in Forgejo

- Status: Proposed
- Date: 2026-08-09

## Context

An unattended agent should be free to commit, branch, and rewrite its own work.
It should not receive a developer credential or direct write access to GitHub
or GitLab.

GitHub or GitLab must remain the source of truth for code, review, CI, and
merges.

## Decision

Add a mediated-agent world mode backed by Forgejo:

```text
GitHub/GitLab -> read-only Forgejo mirror -> per-world Forgejo fork
```

Normal human worlds keep their current Git and SSH-agent behavior.

Each project gets one server-managed mirror. Each agent world gets its own
Forgejo identity, fork, and token. That identity owns only its fork and has no
other repository or organization grants.

WT configures two normal Git remotes:

```text
origin    writable per-world fork
upstream  read-only project mirror
```

The agent may create, force-push, and delete branches in `origin`. It cannot
write the mirror or another world's fork. It receives no GitHub or GitLab
credential.

The token is injected only after the world has its own writable disk. It is not
stored in Git URLs, server state, or logs. Mediated worlds also disable
workstation SSH-agent forwarding. A token-bearing disk is never used as a
shared ancestor or returned to a pool.

Deleting the world first revokes the token and isolates the VM, then removes
the Forgejo fork and identity. Unpublished work is intentionally lost.

ADR 0018 defines how selected work leaves Forgejo. The first implementation
uses normal `wt new`; prewarming comes later.

## Verification

- Two worlds cannot write to each other's forks.
- No external credential or workstation agent socket reaches the world.
- Normal human worlds keep their existing behavior.
- Failed creation and deletion revoke access before discarding the disk.

## Consequences

Agents get an ordinary writable Git remote with limited authority. Forgejo is
disposable infrastructure, not another source of truth.

WT now depends on Forgejo for mediated worlds. `wt-server-setup` owns its
connection and credential configuration.

## Alternatives

External tokens inside worlds are too broad. A shared Forgejo fork lets one
agent damage another's work. Making Forgejo authoritative would duplicate the
existing review and CI system.

## References

- [Forgejo repository mirrors](https://forgejo.org/docs/latest/user/repo-mirror/)
- [Forgejo token scopes](https://forgejo.org/docs/latest/user/token-scope/)
