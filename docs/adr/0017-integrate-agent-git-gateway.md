# ADR 0017: Integrate the agent Git gateway from the WT client

- Status: Proposed
- Date: 2026-08-09

## Context

WT and the agent Git gateway are separate today. Creating a world does not
create a gateway workspace, install its Git credential, publish agent branches,
or revoke Git access when the world is removed.

Doing those steps by hand would make the developer manage two lifecycles for
one disposable environment.

## Developer experience

Configure a gateway once. When creating an agent world, choose the gateway
project and base branch. WT creates the matching Git workspace and installs its
credential before the agent starts.

The agent works with ordinary branch names:

```text
git switch -c fix-login
git push -u origin fix-login
```

The gateway publishes that branch as `df1/fix-login`, using the world name as a
fixed prefix. The same mapping applies to updates, force-pushes, and deletions.
The agent never needs to know the prefix and cannot publish outside it. Tags are
rejected.

Opening a pull or merge request remains a separate action.

Removing the world also revokes its gateway workspace. Branches already
published to GitHub or GitLab remain in the project repository. Gateway worlds
cannot be copied with `wt fork`.

If creation or removal is interrupted, the developer can run it again from any
authorized workstation. WT continues instead of creating a second workspace or
leaving Git access behind.

## Decision

The `wt` client coordinates the WT and gateway operations. The gateway stays a
separate service, and `wt-server` never needs access to Forgejo, GitHub, or
GitLab.

WT passes the world's limited Git credential through the setup SSH connection.
It stays on the world's private disk, outside the checkout, and is not written
to WT state, logs, images, or shared disks. Agent worlds do not forward the
workstation's SSH agent unless the developer explicitly connects with `ssh -A`.

WT removes access before deleting the world. A removal that is waiting on the
gateway leaves the world stopped and inaccessible. Once access is revoked, WT
deletes the private disk.

## Verification

- Creating an agent world also creates and configures its gateway workspace.
- Pushing `fix-login` from `df1` publishes only `df1/fix-login`.
- Updates, force-pushes, and deletions follow the same branch mapping.
- The agent cannot publish outside its prefix or push tags.
- Retrying creation does not create a second gateway workspace.
- Removing a world revokes its Git access before deleting its disk.
- Interrupted creation and removal can be resumed safely.
- The Git credential never appears in WT state, logs, images, shared disks, or
  the checkout.
