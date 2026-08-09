# ADR 0018: Manage one Forgejo fork and identity per agent world

- Status: Accepted
- Date: 2026-08-09

## Context

ADR 0017 prepares exact-commit project generations and claims credential-free
ready worlds. Once claimed, a development agent still needs somewhere to push
its commits. Giving the world a developer's forwarded SSH agent would give
repository and devcontainer code every Git permission held by that developer.
Giving all worlds one shared bot credential would let one compromised world
modify the branches and pull requests of other worlds.

We want agents to work freely inside their own KVM and Git boundaries. An agent
may create, rewrite, and delete branches and push arbitrary commits, but it must
not push to the canonical repository, merge its own change, change repository
permissions, or use another world's Git authority.

Forgejo provides repository forks, pull requests, repository-specific access
tokens, and push mirrors. It calls merge requests pull requests; this ADR uses
"pull request" for the Forgejo object and "MR" when describing the overall
workflow.

This decision treats the canonical Forgejo repository as the write-authoritative
source of truth. GitHub is a downstream publication mirror. Making GitHub
authoritative while accepting merges in Forgejo would require a different
one-way synchronization and conflict model.

## Decision

Integrate Forgejo with prewarm policies. The first implementation supports
Forgejo 15 or newer and does not introduce a generic forge-provider interface.
Supporting another forge requires another decision after its identity,
permission, fork, and pull-request semantics are known.

A Forgejo-enabled prewarm policy names:

- one canonical Forgejo repository;
- one protected base branch, initially `main`;
- the existing warm-generation and resource settings from ADR 0017; and
- the Forgejo installation configured for its WT context.

The canonical repository, rather than its GitHub mirror, supplies the commits
used to build warm generations. Generation and ready-world disks remain free
of agent Git credentials.

An anonymously readable canonical repository uses its normal clone URL during
generation builds. For a private canonical repository, WT mints a temporary
read-only credential limited to that repository, uses it only for the initial
clone, and removes and revokes it before devcontainer lifecycle commands run.
Promotion fails unless the guest and Forgejo both confirm that cleanup. The
temporary builder credential is distinct from every claimed-world token.

### Canonical repository

The canonical Forgejo repository has these invariants:

- its `main` branch rejects direct and force pushes from agent identities;
- only maintainers or a separate merge service can approve and merge pull
  requests;
- required review and CI rules are enforced at merge time;
- WT agent identities have read access but no Code, Actions, administration, or
  merge permission at write level that would make them trusted contributors;
- repository mirror settings are unavailable to WT agent identities.

Pull-request workflows from agent forks retain Forgejo's untrusted-fork
behavior. They require maintainer approval before untrusted changes are allowed
to run with consequential CI authority. WT never promotes an agent identity to
a canonical collaborator to avoid that check.

### Claim a world

Claiming a Forgejo-enabled ready world extends the ADR 0017 claim transaction.
WT first verifies that the ready generation's exact commit is still the
observed head of the canonical base branch. It then:

1. reserves the ready world without exposing its SSH inventory;
2. creates a non-admin Forgejo technical identity named from the immutable
   world ID;
3. creates that identity's fork of the canonical repository and verifies that
   the fork's base branch points at the generation commit;
4. gives the technical identity read-only access to a private canonical
   repository when required;
5. creates a repository-specific token limited to the canonical repository and
   this world's fork, with repository API access but no issue, organization,
   user, package, notification, or administrator scope;
6. installs the token and Forgejo metadata into the claimed disk;
7. configures and verifies the Git remotes and pull-request helper; and
8. publishes the world as running only after both Forgejo and guest state are
   complete.

The technical identity owns its fork but has only read permission on the
canonical repository. The token's route scope and the identity's repository
permission are both required: `write:repository` permits pushes and
pull-request creation where the identity has authority, while canonical
permissions still deny direct pushes, merges, settings changes, and
administration. WT verifies these negative permissions during integration
testing instead of assuming scope names alone are sufficient.

The fork and token are unique to one world. Names and reconciliation keys use
the world UUID, not a user-selected world name. Retried operations discover or
remove partial Forgejo resources rather than creating additional identities or
forks.

If Forgejo setup or guest customization fails, WT revokes the token and removes
the partial fork and technical identity before returning the ready world to the
pool or deleting it. A ready world containing any minted token is never reused.

### Credentials and SSH agent isolation

The Forgejo token is created only after a credential-free ready world has been
claimed. Its plaintext is written directly to a provider-managed credential
mount, is never placed in a Git remote URL, API request record, SQLite value,
progress message, setup log, or environment variable, and is not recoverable
from the active generation or another ready world.

The credential mount is prepared empty in the warm generation and bind-mounted
into the primary devcontainer. Claiming can populate it without recreating the
already-running devcontainer. A Git credential helper and the WT Forgejo helper
read the mode-restricted file when needed. Repository and devcontainer code in
the claimed world can read the token; that is why its remote authority is
narrow and disposable.

Managed SSH inventory sets `ForwardAgent no` for agent worlds. The guest and
app SSH servers also reject agent forwarding, so a user cannot accidentally
make a broader workstation identity available to an agent through a normal WT
connection. Human login still uses the world's authorized public keys.

`wt fork` rejects a Forgejo-managed claimed world in the first implementation.
A disk fork would copy its live token. Supporting it safely requires a
pre-network hook that removes inherited authority, creates a new Forgejo fork
and identity, and installs a replacement credential before the child receives
network access. Prewarm generation forks remain allowed because generations
and ready worlds contain no token.

### Git behavior inside the world

WT uses conventional remote names:

```text
origin    per-world writable Forgejo fork
upstream  canonical read-only Forgejo repository
```

The checked-out `main` branch starts at the exact warm-generation commit.
Fetching and rebasing from `upstream/main` updates it from the canonical
repository. The default push remote is `origin`.

The agent may create any number of branches and may normally push, force-push,
or delete them in `origin`. Those operations affect only its fork. Multiple
agents that need independent authority use separate worlds rather than sharing
one fork.

WT injects a small `wt-pr` helper into the primary devcontainer. It can inspect
the current Forgejo state and create or update a pull request from a named fork
branch to the policy's canonical base branch. It cannot merge or approve a pull
request. The helper is a convenience and not a security boundary; Forgejo
permissions enforce the same result for direct API calls made with the token.

An agent therefore completes work as follows:

1. create commits on one or more local branches;
2. push those branches to `origin` as often as needed;
3. use `wt-pr` to create an MR for a selected branch;
4. continue pushing to that branch to update the MR; and
5. leave review, CI approval, and merge to a canonical maintainer.

WT records the Forgejo fork URL and discovered pull-request URLs as world
metadata so `wt ls` and `wt get` can report them. Forgejo remains authoritative
for pull-request state.

### World deletion and external cleanup

Deleting a managed world first removes its ability to reach Forgejo and revokes
its token. Only then may WT destroy its VM disk. If Forgejo is unavailable, WT
stops or network-isolates the VM, records cleanup as pending, and retries; it
does not leave a running world with an unrevoked credential after reporting
successful deletion.

If the fork has no open pull request, WT deletes the fork and technical identity
with the world. Unpushed commits and pushed branches without an open pull
request are intentionally discarded by `wt rm`.

If the fork has an open pull request, WT revokes the token and disables the
technical identity but retains the fork so reviewers can still inspect and
merge its refs. Reconciliation removes the retained fork and identity after all
of its pull requests are merged or closed. Deletion progress identifies any
retained review resources.

### Forgejo-to-GitHub publication

After a one-time migration, the canonical Forgejo repository owns `main`.
Direct GitHub writes are disabled operationally, and contributors open pull
requests in Forgejo.

Forgejo owns a push mirror from the canonical repository to the corresponding
GitHub repository. The mirror:

- is restricted to `main` rather than using an unrestricted `git push
  --mirror`;
- synchronizes on new canonical commits and retries periodically;
- uses a GitHub credential held by Forgejo, never WT or an agent world; and
- reports lag or failure without rolling back an accepted Forgejo merge.

Only canonical refs are published. Per-world forks, agent branches, Forgejo
pull-request metadata, reviews, and CI state are not mirrored to GitHub. Tags
and releases need a separate publication policy if they are later required.

The cutover from an existing GitHub repository imports and verifies the
expected `main` commit in Forgejo, freezes GitHub writes, enables the one-way
push mirror, and verifies the same commit at both endpoints. WT does not enable
bidirectional mirroring.

### Component ownership

- `wt` selects Forgejo-enabled policies and reports fork and pull-request
  metadata.
- `wt-server` owns typed Forgejo lifecycle state, idempotent external
  reconciliation, claim ordering, token revocation, and cleanup decisions.
- `wt-provider` owns the empty credential mount, claim-time Git configuration,
  agent-forwarding policy, and guest verification.
- `wt-guest` owns the injected `wt-pr` helper and token-safe API invocation.
- `wt-libvirt` remains unaware of Forgejo, Git remotes, and tokens.
- `wt-server-setup` installs and validates the Forgejo endpoint and the
  mode-`0600` server credential file. The credential is never stored inline in
  runtime TOML.
- Forgejo owns canonical permissions, technical identities, forks, pull
  requests, branch protection, Actions trust, and the GitHub push mirror.
- Real Forgejo and KVM lifecycle tests belong in `wt-integration-tests`.

Forgejo is an external service. WT does not install, upgrade, back up, or run
Forgejo or its database.

## Verification

- Two worlds for the same project receive different Forgejo identities, forks,
  and tokens.
- A world can create, update, force-push, and delete branches in its own fork.
- A world can create and update multiple pull requests against canonical
  `main`.
- A world cannot push any canonical branch, merge or approve a pull request,
  change canonical settings, access another private fork, or administer
  Forgejo.
- A fork pull request remains untrusted for Forgejo Actions until a canonical
  maintainer approves it under the configured policy.
- No workstation agent socket is available through guest or app SSH.
- No token exists in a warm generation or unclaimed ready world, and claiming
  one world does not change its siblings.
- Token plaintext is absent from API records, SQLite, Git configuration,
  process arguments, progress output, and setup logs.
- Failed claims and interrupted retries leave at most reconcilable,
  non-published Forgejo resources.
- World deletion revokes access before deleting the disk and retains forks only
  while open pull requests need them.
- A merge to canonical `main` appears at the exact same commit on GitHub, and a
  GitHub-side write never flows back into Forgejo.
- Existing non-Forgejo worlds retain their current SSH-agent behavior.

## Consequences

Agents get broad Git freedom within a disposable remote fork without receiving
a developer credential or canonical write authority. Forgejo supplies a
durable review boundary outside the VM, while KVM and per-world Docker state
remain the execution boundary.

WT gains external transactional state. Creating and deleting a world now spans
SQLite, libvirt, the guest, and Forgejo, so all Forgejo mutations require
durable reconciliation and explicit partial-failure tests.

The WT server holds a powerful Forgejo management credential because it must
create and disable technical identities and their resources. The host is
already trusted by WT's safety model, but compromise now also affects the
configured Forgejo installation. The credential must be dedicated, audited,
mode-restricted, redacted, and limited by Forgejo as far as its management API
allows.

The per-world token is intentionally readable by the agent and trusted project
code. Isolation comes from making that credential unique, narrow, revocable,
and unable to merge, not from trying to hide it from code running as the
devcontainer user.

Forgejo becomes the operational source of truth. GitHub remains useful for
distribution and discovery, but GitHub pull requests, reviews, and direct
commits are outside the accepted workflow.

## Alternatives

### Forward the developer's SSH agent

Rejected because arbitrary agent and repository code could use every identity
offered by the workstation agent, including access unrelated to this world.

### Give every world one shared bot account and token

Rejected because one world could modify another world's branches and pull
requests, and revocation would interrupt all worlds.

### Give the agent canonical write access and rely on branch protection

Rejected because permission mistakes, alternate refs, API capabilities, or a
future protection change could turn an agent compromise into a canonical
write. The fork and read-only canonical identity provide an independent
authorization boundary.

### Use only write-enabled deploy keys

Rejected as the complete solution because a deploy key can isolate Git pushes
to one fork but does not provide the API identity needed to create and update
pull requests. It remains a possible Git transport implementation if paired
with an equally narrow pull-request broker.

### Use Forgejo AGit without forks

Rejected because AGit can create pull requests through a special push ref but
does not provide the explicit, inspectable per-world remote namespace required
for arbitrary branches and durable agent output.

### Copy a token into the warm generation

Rejected because every copy-on-write descendant would inherit the same secret.
Credential injection happens only after a unique ready world is claimed.

### Keep GitHub authoritative with a Forgejo pull mirror

Rejected for this workflow because reviews and merges occur in Forgejo. Pulling
GitHub back into the canonical branch would create two write paths and unclear
conflict ownership. Reversing authority requires a separate design in which
Forgejo does not merge canonical changes directly.

### Build a generic forge abstraction first

Rejected because only Forgejo's current permission and token semantics have
been selected. A premature common interface could hide security differences
between forge implementations.

## References

- [Forgejo access-token scopes](https://forgejo.org/docs/latest/user/token-scope/)
- [Forgejo pull requests and Git flow](https://forgejo.org/docs/latest/user/collaboration/pull-requests-and-git-flow/)
- [Forgejo security for pull requests from forks](https://forgejo.org/docs/latest/user/actions/security-pull-request/)
- [Forgejo repository mirrors](https://forgejo.org/docs/latest/user/repo-mirror/)
