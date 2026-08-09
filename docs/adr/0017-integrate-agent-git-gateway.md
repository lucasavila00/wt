# ADR 0017: Add gateway-backed Git access to agent worlds

- Status: Proposed
- Date: 2026-08-09

## Context

WT relies on the developer's forwarded SSH agent for all Git access. It uses the
agent to clone the repository during setup and forwards it into the devcontainer
for later Git commands.

An agent therefore uses the developer's Git identity and permissions. WT cannot
give the agent limited access or revoke that access independently.

## Developer experience

Configure the gateway in WT once. When creating an agent world, choose a
project and base branch in addition to the world name.

For a world named `df1`, WT creates a private Forgejo fork for that project and
world name. It checks out the selected base branch, configures the fork as
`origin`, and installs its credential. The agent does not need the developer's
SSH agent to use this remote.

The agent uses ordinary branch names inside the world:

```text
git switch -c fix-login
git push -u origin fix-login
```

The gateway publishes `origin/fix-login` to the project's GitHub or GitLab
repository as `df1/fix-login`. The `df1/` prefix comes from the world name and
is added by the gateway. Pushing more commits, force-pushing, or deleting
`origin/fix-login` updates the same branch in the project repository.

The agent cannot publish a branch without the `df1/` prefix, publish under
another world's prefix, or push tags. Opening a pull or merge request remains a
separate action.

Running `wt rm df1` deletes the world as usual. It does not delete the Forgejo
fork, its branches, its identity, or its authorized public key. The private key
installed by WT disappears with the world's disk.

Creating another `df1` for the same project reuses the retained fork and
installs a new credential. Existing credentials remain authorized. `wt fork`
rejects agent worlds because it must not copy a Git credential or identity.

## Decision

The gateway runs as a separate service. When creating an agent world, the `wt`
client calls the gateway API to create or find the private Forgejo fork for the
project and world name. The gateway manages Forgejo and publishes branches to
GitHub or GitLab. The world connects to Forgejo through Git. `wt-server` does
not call the gateway or any Git provider.

WT sends the fork credential through the setup SSH connection. The credential
stays on the world's private disk, outside the checkout. WT does not write it to
its database, logs, VM images, or shared disks.

Agent worlds do not forward the workstation's SSH agent during setup or normal
connections. A developer can still opt in for a connection with `ssh -A`.
Normal worlds keep their existing agent-forwarding behavior.

`wt rm` uses the existing server deletion path and does not contact the gateway.
Retained forks accumulate by design so work can be recovered after an accidental
world deletion.
