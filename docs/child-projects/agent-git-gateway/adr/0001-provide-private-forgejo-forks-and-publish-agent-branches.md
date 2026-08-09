# ADR 0001: Provide private Forgejo forks and publish agent branches

- Status: Proposed
- Date: 2026-08-09

## Context

Agents need Git access without receiving the developer's identity or the
credentials that manage repositories on GitHub or GitLab.

They should use normal Git commands and branch names while the gateway keeps
their work inside one private fork and one external branch namespace.

## Decision

GitHub or GitLab remains the source of truth. The gateway keeps one read-only
Forgejo mirror for each project and one private Forgejo fork for each project
and namespace.

When a client requests the namespace `df1`, it sends the project, base branch,
and a public key. The gateway:

1. Refreshes the project mirror.
2. Creates the private fork from the selected base, or reuses the existing
   `df1` fork without resetting it.
3. Authorizes the public key.
4. Returns remote URLs with no embedded credential:

```text
origin    private writable Forgejo fork
upstream  read-only Forgejo project mirror
```

The client creates the keypair. The gateway never receives the private key.
Requesting `df1` again reuses the fork and adds another public key. Existing
keys remain authorized.

The public key grants Git access only: it can read the project mirror and write
branches in the private fork. It cannot access another fork or use Forgejo's API,
web UI, or administration features. Tags are rejected. Agent code does not run
as Forgejo Actions or hooks.

## Publishing branches

The agent uses ordinary branch names in its private fork. For `df1`, pushing
`fix-login` publishes this branch directly to GitHub or GitLab:

```text
Forgejo fork   fix-login
GitHub/GitLab  df1/fix-login
```

Later pushes, force-pushes, and deletions update that same external branch. The
gateway chooses the external project and adds the `df1/` prefix; the agent
cannot change either.

The gateway refuses a namespace already owned by another fork and never
overwrites an unrelated external branch. Its GitHub or GitLab credentials stay
in the gateway and can write only branches under the namespace.

Opening a pull or merge request remains a separate action.

## Retention

The gateway does not revoke keys or delete private forks. Forks, branches,
identities, and public keys remain. Any copy of an authorized private key also
remains usable.
