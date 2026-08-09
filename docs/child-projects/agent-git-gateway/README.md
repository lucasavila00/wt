# Agent Git gateway

This is a temporary home for a standalone tool that gives agents a disposable
Git workspace and narrowly controlled publication to GitHub or GitLab.

The gateway knows about projects, workspaces, refs, and assignments. It does
not know about WT, VMs, devcontainers, or any other runner. A runner creates a
workspace, gives its agent the returned Git access, and revokes the workspace
when the job ends.

These documents can move to their own repository later without changing that
boundary.

## Flow

1. A trusted runner selects a project and base ref.
2. The gateway syncs its read-only mirror and creates a provisional workspace.
3. The runner gives the workspace's Git-only credential to its agent.
4. The agent works freely in the private fork.
5. Branches under the workspace's reserved prefix are assigned automatically.
6. The gateway pins each commit, updates its leased staging ref, and opens or
   updates its review.
7. The runner isolates the workload. The gateway fences Git and publication,
   then revokes the workspace. Published review state remains.

## Decisions

- [ADR 0001: Isolate agent Git work in Forgejo](./adr/0001-isolate-agent-git-work-in-forgejo.md)
- [ADR 0002: Publish agent branch namespaces](./adr/0002-publish-agent-branch-namespaces.md)
