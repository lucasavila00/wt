# ADR 0017: Integrate the agent Git gateway from the WT client

- Status: Proposed
- Date: 2026-08-09

## User flow

The user configures a gateway once. In `wt new`, they choose an agent world,
project, and base branch. WT creates the VM, private Git fork, and checkout.

The world appears in `wt ls` and works with `ssh NAME` and `wt code NAME`. The
agent may commit, branch, and push inside its fork.

`wt publish NAME` asks for a source branch and target base. It creates one
assignment and returns the GitHub or GitLab review URL. Later pushes to that
branch update the same review. Other branches stay private.

`wt rm NAME` revokes the Git workspace and deletes the world. Tasks continue to
use normal SSH or agent tooling.

## Decision

The gateway stays standalone. The `wt` client calls it directly. `wt-server`
never calls Forgejo, GitHub, or GitLab.

`wt-server` records the gateway, workspace, and operation IDs; the exact initial
commit; token-free remotes; and one state: `creating`, `ready`, `revocation
pending`, or `cleanup pending`.

Each external operation uses a server-issued expiring claim and monotonic
revision. Stale or duplicate calls have no effect. Another authorized
workstation can resume an interrupted operation.

Creation reserves the WT world, creates a provisional gateway workspace,
attaches it to the WT record, and provisions the guest. Unattached gateway
leases expire.

The client sends the Git-only workspace credential over setup SSH stdin. The
guest stores it outside the checkout in a mode-`0600` file on the world's
private disk and mounts it read-only into the app. Git configuration contains
only token-free remotes and the credential-helper path. Reissuing a credential
invalidates the previous one.

Gateway worlds default to `ForwardAgent no` and receive no agent socket during
setup. A user may explicitly connect with `ssh -A`. Normal worlds keep the ADR
0003 behavior.

The WT client calls the gateway assignment API for `wt publish`; the workspace
credential cannot call it.

Deletion stops the VM, removes SSH access, and records `revocation pending`.
The gateway fences Git and publication before confirming revocation. WT then
records `cleanup pending` and deletes the private disk.

`wt fork` rejects gateway worlds before any disk pivot.

## Verification

- Creation, publication, restart, and deletion work without manual credentials.
- Interrupted and competing operations converge safely.
- Credentials never enter WT state, logs, images, or shared disks.
- Revocation-pending worlds stay isolated; cleanup-pending worlds cannot push,
  publish, or fork.
