# ADR 0017: Integrate the agent Git gateway from the WT client

- Status: Proposed
- Date: 2026-08-09

## User flow

The user configures a gateway once. In `wt new`, they choose an agent world,
project, and base branch. WT creates the VM, private Git fork, checkout, and a
branch prefix based on the world name. A world named `df1` always gets `df1/`.
The prefix is not configurable.

The world appears in `wt ls` and works with `ssh NAME` and `wt code NAME`. The
agent may commit, branch, and push inside its fork. Pushing `df1/fix-a`
automatically creates or updates one review against the configured base.
`df1/fix-b` gets another review. Branches outside `df1/` stay private.

`wt reviews NAME` lists the review URLs. `wt rm NAME` revokes the Git workspace
and deletes the world. Tasks continue to use normal SSH or agent tooling.

## Decision

The gateway stays standalone. The `wt` client calls it directly. `wt-server`
never calls Forgejo, GitHub, or GitLab.

`wt-server` records the gateway, workspace, and operation IDs; exact initial
commit; branch prefix; target base; token-free remotes; and one state:
`creating`, `ready`, `revocation pending`, or `cleanup pending`.

Each external operation uses a server-issued expiring claim and monotonic
revision. Stale or duplicate calls have no effect. Another authorized
workstation can resume an interrupted operation.

Creation reserves the WT world, creates a provisional gateway workspace and
publication grant, attaches them to the WT record, and provisions the guest.
Unattached gateway leases expire.

The client sends the Git-only workspace credential over setup SSH stdin. The
guest stores it outside the checkout in a mode-`0600` file on the world's
private disk and mounts it read-only into the app. Git configuration contains
only token-free remotes and the credential-helper path. Reissuing a credential
invalidates the previous one.

Gateway worlds default to `ForwardAgent no` and receive no agent socket during
setup. A user may explicitly connect with `ssh -A`. Normal worlds keep the ADR
0003 behavior.

The gateway creates one exact assignment for every matching branch and
reconciles later pushes. The workspace credential cannot change the prefix,
base, or assignments.

Deletion stops the VM, removes SSH access, and records `revocation pending`.
The gateway fences Git and publication before confirming revocation. WT then
records `cleanup pending` and deletes the private disk.

`wt fork` rejects gateway worlds before any disk pivot.

## Verification

- Matching branches create distinct reviews; other branches remain private.
- Creation, restart, review listing, and deletion need no manual credentials.
- Interrupted and competing operations converge safely.
- Credentials never enter WT state, logs, images, or shared disks.
- Revocation-pending worlds stay isolated; cleanup-pending worlds cannot push,
  publish, or fork.
