# ADR 0002: Sync agent branch namespaces

- Status: Proposed
- Date: 2026-08-09

## Decision

Workspace creation includes a branch-sync grant containing:

- one external project and staging fork;
- one branch prefix reserved for that workspace;
- whether history rewrites are allowed; and
- service limits for branch updates.

The prefix is the workspace name followed by `/`. It is immutable and not
configurable. A workspace named `df1` always gets `df1/`. Workspace creation
fails if that prefix cannot be reserved for the project.

Every branch under the prefix syncs automatically to the external staging
fork. Creating, updating, force-pushing, or deleting the Forgejo branch performs
the same operation on its external counterpart. Other branches and tags are
rejected by ADR 0001.

For each branch, the gateway stores an exact mapping to one external staging
ref and its expected commit. It atomically pins each source commit before
transfer and updates the staging ref with a lease. Any project, ref, or lease
mismatch is a conflict.

The workspace Git credential cannot use the control plane or change its grant.
Only a trusted project operator can create, change, or revoke a grant.

Mirror reads and staging writes use separate external credentials. The staging
credential cannot write canonical branches or manage reviews, merges, secrets,
hooks, workflows, or repository access.

Staging-fork CI is untrusted and receives no protected secrets, write token,
privileged runner, or trusted-branch treatment.

Revocation disables the grant, rejects queued updates, and cancels or drains
in-flight sync before acknowledging the fence. Existing external branches stay
in the staging fork unless the agent deleted them before revocation.

Creating a pull or merge request from a staging branch to the canonical project
requires a separate decision and permission.

## Verification

- Matching branches create, update, and delete only their mapped staging refs.
- Non-matching branches and tags cannot be pushed.
- Branch rewrites after pinning do not change the transferred commit.
- Agents cannot change their grant or obtain external credentials.
- Sync never creates or changes a pull or merge request.
