# ADR 0017: Add gateway-backed Git access to agent worlds

- Status: Proposed
- Date: 2026-08-09

## Context

WT currently uses the developer's forwarded SSH agent for every Git operation:
the initial clone and later fetches and pushes from the devcontainer. An agent
inside the world therefore uses the developer's Git identity and permissions.

Agent worlds need their own Git identity, limited to their own fork and branch
namespace. Normal worlds should keep using SSH agent forwarding as they do
today.

## Decision

Add an agent Git mode to world creation. Configure the gateway in WT once, then
select a project and base branch when creating an agent world.

For an agent world named `df1`:

1. The `wt` client generates a new SSH keypair.
2. The client asks the gateway for the Forgejo fork identified by the project
   and world name. The gateway creates the fork if needed and authorizes the
   public key.
3. The client sends `wt-server` an agent-mode create request containing the
   Forgejo remotes, selected base, and private key.
4. `wt-server` creates the world, stores the private key on its private disk,
   checks out the selected base, and configures the remotes.

`origin` is the world's private, writable Forgejo fork. `upstream` is the
gateway's read-only mirror of the project. The agent can use both without the
developer's SSH agent.

The server is trusted in WT's self-hosted model, so sending the world-specific
private key through the existing protected API is an acceptable and simpler
design. `wt-server` may handle the key while creating the world but never writes
it to its database, logs, VM images, or shared disks. Its only persistent copy
is on the world's private disk, outside the checkout.

## Branches

The agent uses ordinary branch names:

```text
git switch -c fix-login
git push -u origin fix-login
```

For `df1`, the gateway publishes the fork's `fix-login` branch to the project's
GitHub or GitLab repository as `df1/fix-login`. The prefix comes from the world
name and is added automatically. Later pushes, force-pushes, and deletions
update that same published branch.

The agent cannot publish outside `df1/`, publish under another world's prefix,
or push tags. Opening a pull or merge request remains a separate action.

## Removing and reusing worlds

`wt rm df1` uses the existing server deletion path. It does not contact the
gateway or change anything in Forgejo. Deleting the world's private disk removes
WT's copy of the private key.

The Forgejo fork, its branches, identity, and authorized public keys remain.
This is deliberate: the data is small, and retaining it makes recovery from an
accidental world deletion possible. Any copy of an old private key also remains
valid because WT does not revoke its public key.

Creating another `df1` for the same project reuses the retained fork and adds a
new credential. Retained forks and credentials accumulate by design.

`wt fork` rejects agent worlds because copying the disk would also copy their
Git identity and private key.

## Boundaries

- The `wt` client calls the gateway only when creating an agent world.
- The gateway manages Forgejo and publishes branches to GitHub or GitLab.
- The world connects to Forgejo through normal Git transport.
- `wt-server` handles the supplied private key but never calls the gateway,
  Forgejo, GitHub, or GitLab.
- Agent worlds do not forward the workstation's SSH agent by default. A
  developer can still opt in for a connection with `ssh -A`.
