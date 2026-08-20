# ADR 0042: Share agent conversations between worlds

- Status: Accepted
- Date: 2026-08-20
- Amended by: [ADR 0043](0043-own-retained-guest-foundation-in-shared-images.md)

> [!WARNING]
> The Codex discovery claim in this ADR is incomplete. Sharing
> `~/.codex/sessions` preserves rollout files, but an already-running Codex
> process does not add them to its local session index or show them in its
> picker. [ADR 0044](0044-reconcile-shared-codex-sessions.md) defines the
> required Codex-owned reconciliation step. Do not share or edit Codex's
> SQLite state to work around this limitation.

## Context

Deleting a world currently deletes its Codex conversations. We want those
conversations to survive and appear in every retained world on the same WT
server.

WT needs to keep `~/.codex/sessions` in sync across retained worlds. Everything
else in the Codex folder stays local to each world.

## Decision

Let the server map a host folder into the home directory of every retained
world:

```toml
[[shared_folders]]
source = "/home/wt/.codex/sessions"
target = ".codex/sessions"
```

`source` is a folder on the WT server. `target` is a path inside the world
user's home. WT makes the same server folder appear there in every VM. The VM
mount uses virtiofs. The retained image foundation gives the `wt` user and
group UID/GID `1001:1001` in every VM. Server setup grants both that UID and the
installing server user access to shared sources with POSIX ACLs.

A devcontainer does not receive the folder automatically. Its repository can
add a normal bind mount from the VM path to the container user's home.

Configure one folder for `.codex/sessions`. Do not share the rest of the Codex
home directory.

Only use a conversation in one world at a time. Two worlds writing to the same
conversation file could damage it. CI worlds do not receive these folders.

## Consequences

Conversations survive world deletion and appear in other worlds. Other agent
files stay private to each world.

Shared folders are not included in world snapshots or disk quotas. The server
owner must back them up separately. We need KVM tests for both regular worlds
and devcontainer-world VMs.

Existing worlds are not migrated when this ownership contract changes. Recreate
affected worlds to use a newly built image.
