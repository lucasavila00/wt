# ADR 0001: Isolate agent Git work in Forgejo

- Status: Proposed
- Date: 2026-08-09

## Decision

The gateway provides isolated Git workspaces backed by Forgejo:

```text
GitHub/GitLab -> read-only Forgejo mirror -> private workspace fork
```

GitHub or GitLab remains the source of truth.

Each project has one gateway-managed mirror. Each workspace has its own fork
and Git identity. That identity can write its fork and read the mirror. It
cannot access another workspace or use the Forgejo API or web UI.

The identity cannot create or administer repositories, keys, collaborators,
hooks, Actions, packages, or mirrors. Forgejo Actions and agent-controlled hooks
are disabled on mirrors and workspace forks.

The gateway returns:

```text
origin    writable workspace fork
upstream  read-only project mirror
```

The agent may create, rewrite, and delete branches in `origin`. It receives no
GitHub, GitLab, or mirror credential.

The runner may persist the workspace credential only in private state that is
never cloned or reused. Reissuing it invalidates the previous credential.

Revocation fences active and new Git sessions, applies the publication rules in
ADR 0002, and then removes the identity and fork. Published work remains;
unpublished work is deleted.

## Verification

- Workspaces cannot read or write each other's forks.
- Workspace credentials provide Git access only.
- Pushing workflow or hook files executes nothing on the gateway.
- Reissued and revoked credentials stop working.
