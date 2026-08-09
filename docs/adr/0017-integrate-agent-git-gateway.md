# ADR 0017: Integrate the agent Git gateway from the WT client

- Status: Proposed
- Date: 2026-08-09

## Developer experience

Configure the Git gateway once, then create an agent world with `wt new`. Choose
the project and base branch as part of the normal creation flow. WT creates the
VM, gives it a private Git fork, checks out the requested base commit, and opens
the world like any other WT world.

From then on, the usual commands work:

```text
ssh df1
wt code df1
wt ls
```

Git is ready when the world starts. `origin` is the private, writable fork and
`upstream` is the read-only project mirror. There are no Git tokens to paste and
no SSH agent to configure.

The world owns one branch namespace derived from its name. For example, `df1`
may create, rewrite, and delete branches under `df1/`:

```text
git switch -c df1/fix-login
git push -u origin df1/fix-login
```

The gateway copies every `df1/*` branch to the project's GitHub or GitLab
repository. Updates, force-pushes, and deletions follow automatically. Pushes
to any other branch, and all tag pushes, are rejected.

Opening a pull or merge request is a separate action. This integration only gets
the agent's branches into the project repository.

When the work is finished, run:

```text
wt rm df1
```

WT first cuts off the world's Git access and branch syncing, then deletes its
private disk. Branches already copied to the project repository remain there.
Gateway worlds cannot be duplicated with `wt fork`.

## How WT provides this

The gateway remains a standalone service, called directly by the `wt` client.
`wt-server` never talks to Forgejo, GitHub, or GitLab.

During creation, WT reserves the world, asks the gateway for a provisional Git
workspace and sync grant, records them with the world, and provisions the VM.
The record includes the exact starting commit, branch prefix, base branch,
token-free remotes, gateway operation IDs, and cleanup state. Provisional
gateway workspaces expire if they are never attached to a world.

The client passes the Git-only credential over the setup SSH connection. The
guest stores it outside the checkout in a mode-`0600` file on the world's
private disk and mounts it read-only into the devcontainer. Git configuration
contains only token-free remotes and the credential-helper path. Reissuing the
credential invalidates the old one.

Gateway worlds do not forward the workstation's SSH agent during setup and use
`ForwardAgent no` by default. A developer can still opt in for a connection with
`ssh -A`. Normal worlds keep their existing agent-forwarding behavior.

Creation and deletion are resumable from another authorized workstation. The
server records whether the world is `creating`, `ready`, `revocation pending`,
or `cleanup pending`. Gateway calls use short-lived server claims and increasing
revision numbers, so retrying an operation cannot undo newer work.

Deletion deliberately happens in this order:

1. Stop the VM and remove SSH access.
2. Have the gateway block Git access and finish or cancel branch sync.
3. Delete the world's private disk.

A world waiting for gateway revocation stays stopped and isolated. Once
revocation succeeds, it cannot push or sync even if disk cleanup is interrupted.

## Verification

- A new gateway world can clone, fetch, and push without manual Git credentials.
- Branches under the world's prefix sync to the project repository; other
  branches and tags are rejected.
- Restarting a world does not require new credentials.
- Interrupted or competing create and delete operations converge on one state.
- Credentials never enter WT state, logs, images, or shared disks.
- Worlds awaiting revocation remain isolated. Worlds awaiting disk cleanup
  cannot push, sync, or fork.
