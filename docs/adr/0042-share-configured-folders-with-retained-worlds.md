# ADR 0042: Share agent conversations between worlds

- Status: Accepted
- Date: 2026-08-20

## Context

Deleting a world currently deletes its Codex and Claude Code conversations.
We want those conversations to survive and appear in every retained world on
the same WT server.

WT needs to keep `~/.codex/sessions` and `~/.claude/projects` in sync across
retained worlds. Everything else in the Codex and Claude Code folders stays
local to each world.

## Decision

Let the server map a host folder into the home directory of every retained
world:

```toml
[[shared_folders]]
source = "/var/lib/wt/shared/codex-sessions"
target = ".codex/sessions"
```

`source` is a folder on the WT server. `target` is a path inside the world
user's home. WT makes the same server folder appear there in every VM. The VM
mount uses virtiofs.

A devcontainer does not receive the folder automatically. Its repository can
add a normal bind mount from the VM path to the container user's home.

Configure one folder for `.codex/sessions` and another for `.claude/projects`.
Do not share the rest of either agent's home directory.

Only use a conversation in one world at a time. Two worlds writing to the same
conversation file could damage it. CI worlds do not receive these folders.

## Consequences

Conversations survive world deletion and appear in other worlds. Other agent
files stay private to each world.

Shared folders are not included in world snapshots or disk quotas. The server
owner must back them up separately. We need KVM tests for both regular worlds
and devcontainer-world VMs.
