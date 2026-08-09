# ADR 0018: Use Forgejo as a per-agent Git capability proxy

- Status: Accepted
- Date: 2026-08-09

## Context

ADR 0017 prepares credential-free, exact-commit project generations and claims
ready worlds. Once claimed, an unattended development agent needs to push
commits and update its assigned pull request without receiving the developer's
Git credentials or general write access to a repository.

The repositories where work already happens remain on GitHub or GitLab. Their
branches, pull or merge requests, review state, CI, and merges must remain
authoritative there. Forgejo is useful as a private Git proxy where WT can give
each world broad freedom without giving the world any GitHub or GitLab
credential.

GitHub and GitLab credentials are repository-scoped, not branch- or
pull-request-scoped. Giving one directly to an agent would grant more authority
than its assignment. Branch protection reduces that risk but is not an
independent capability boundary.

We want a world to be safe to leave unattended. An agent may create, rewrite,
and delete arbitrary branches in its private proxy fork. Only explicitly
assigned branches may cross into GitHub or GitLab, and only explicitly assigned
pull or merge requests may be created or updated. Agents never approve, merge,
or change canonical repository settings.

## Decision

Add a Git mediation layer to Forgejo-enabled prewarm policies:

```text
GitHub or GitLab canonical repository
              │ read synchronization
              ▼
      Forgejo upstream proxy
              │ fork at world claim
              ▼
       per-world Forgejo fork  ◄── agent pushes freely
              │ assigned refs only
              ▼
          WT publisher
              │ exact ref update and PR/MR API calls
              ▼
 GitHub or GitLab staging fork and assigned PR/MR
```

GitHub or GitLab is the source of truth. Forgejo stores disposable agent work
and supplies the narrow credential exposed inside a world. WT is the policy
enforcement point between them.

The first implementation uses Forgejo 15 or newer as the proxy and supports
GitHub and GitLab as authoritative forges. Because two external forges are in
scope, their adapters implement one deliberately small capability interface:

- observe exact repository and ref state;
- fetch objects needed by the upstream proxy;
- update one previously assigned head ref with a compare-and-swap lease;
- create or update one previously assigned pull or merge request; and
- read publication, review, and CI status.

The interface has no merge, approval, repository-settings, collaborator,
workflow-secret, release, tag, or arbitrary-ref operation.

### Upstream proxy and warm generations

Each configured external repository has one WT-managed, read-only Forgejo
upstream proxy. The reconciler copies canonical refs from GitHub or GitLab into
that repository. Agent identities cannot push to it and it never pushes back to
the external forge.

ADR 0017 generations track a configured branch in this proxy, normally `main`.
Before promoting or claiming a generation, WT verifies that its exact commit is
still the observed external canonical head. A stale proxy cannot make a stale
ready world eligible.

External read credentials remain on the WT server. They are used only by the
typed external-forge adapter and are never stored in Forgejo clone URLs, warm
disks, or agent worlds. Public repositories need no read credential.

### Assignments are capabilities

An assignment is durable WT state containing:

- an immutable assignment ID and owning world ID;
- the external forge and immutable repository identity;
- one canonical base ref;
- one WT-managed external staging repository and exact head ref;
- one exact proxy-fork source ref;
- the expected external head commit used as a force-with-lease value;
- an optional existing pull or merge request ID; and
- whether non-fast-forward updates are allowed for this head ref.

A world may have several assignments, but every mapping is explicit. There are
no repository-wide or branch-pattern capabilities. Creating another branch in
the Forgejo fork does not grant permission to publish it.

Assignments are created or changed only through the authenticated WT client and
control plane. An agent may request another assignment, but cannot approve or
materialize that request with its world credential.

For new work, WT allocates a namespaced external staging branch such as
`wt/<assignment-uuid>` and opens a pull or merge request from it. For existing
work, WT may attach to an existing PR or MR only when its head repository and
ref are writable by the configured publisher and the user explicitly transfers
that head to WT. WT records its current commit as the initial lease. It does not
take over a human-controlled branch silently.

The external head lives in a dedicated WT staging fork, not the canonical
repository. This keeps agent changes out of canonical branch namespaces and
preserves the external forge's untrusted-fork CI boundary. One staging fork may
hold many UUID-namespaced assignment branches; agents never receive its
credential.

### Claim a world

Claiming a Forgejo-enabled ready world extends the ADR 0017 claim transaction.
WT:

1. reserves the ready world without exposing its SSH inventory;
2. creates a non-admin Forgejo technical identity named from the world UUID;
3. creates that identity's fork of the upstream proxy at the generation commit;
4. creates a repository-specific Forgejo token limited to that fork and
   read-only access to its upstream proxy;
5. installs the token, assignment metadata, and Git helpers into the claimed
   disk; and
6. publishes the world only after Forgejo, Git, guest, and app checks pass.

The token has repository API access where the technical identity has authority,
but no issue, organization, user, package, notification, or administrator
scope. The identity owns only its proxy fork. WT verifies that it cannot write
the upstream proxy or another world's private fork.

The token is minted only after a credential-free ready world is reserved. Its
plaintext is written directly to a provider-managed credential mount and is
never placed in a Git remote URL, SQLite value, API request record, process
argument, progress message, or setup log. The mount is prepared empty in the
warm generation so claim-time injection does not recreate the running
devcontainer.

Repository and devcontainer code can read the token as the devcontainer user.
That is acceptable because its authority ends at this world's disposable
Forgejo fork.

### Git behavior inside the world

WT configures conventional remotes:

```text
origin    per-world writable Forgejo fork
upstream  read-only Forgejo proxy of the GitHub or GitLab repository
```

The default push remote is `origin`. The agent may push, force-push, or delete
any branches there. It can fetch and rebase from `upstream` without contacting
the external forge directly.

WT injects a `wt-pr` helper that lists the world's assignments, validates the
local source branch, pushes it to the assigned proxy ref, and reports the last
external publication and PR or MR status. It cannot create an assignment,
change a destination ref, approve, or merge. Direct Forgejo operations may
bypass the helper but do not bypass the publisher's assignment checks.

Managed SSH inventory sets `ForwardAgent no`. Guest and app SSH servers also
reject agent forwarding for these worlds. A normal human connection therefore
cannot accidentally expose a broader workstation identity to the agent.

`wt fork` rejects a claimed mediated world initially because its disk contains
a live Forgejo token and assignment identity. Prewarm generation forks remain
safe because credentials are injected only after claim. Supporting claimed
world forks requires a separate pre-network credential and assignment rebinding
phase.

### Publish an assigned ref

The reconciler observes assignment refs in each world-owned Forgejo fork. When
an assigned source ref changes, the publisher:

1. loads the assignment by immutable ID rather than accepting a destination
   repository or ref from the agent;
2. verifies that the source commit exists at the assigned proxy ref;
3. reads the current external staging ref and requires it to match the stored
   lease;
4. obtains a short-lived or otherwise narrowly scoped server-held credential;
5. transfers only the required Git objects;
6. updates exactly the assigned external ref, using force-with-lease when the
   assignment permits history rewrites;
7. creates the assigned PR or MR if it does not exist, or verifies that its
   base and head still match the assignment; and
8. records the published commit, external URL, and observed review and CI
   state.

The publisher never uses `git push --mirror`, a wildcard refspec, an
agent-supplied destination, or an unleased force push. Other branches and tags
in the proxy fork remain private to that world.

If the external head changed outside WT, publication stops with a conflict. WT
does not overwrite the change or silently move its lease. A trusted user must
adopt the new head or create another assignment.

The initial external capability permits commit updates, PR or MR creation, and
updates to that object's title and description. It does not permit closing,
merging, approving, changing reviewers or labels, editing other PRs or MRs, or
deleting the external branch. Additional operations require explicit future
capabilities rather than broader API pass-through.

### External-forge credentials

No GitHub or GitLab credential enters a world or Forgejo proxy repository.
`wt-server-setup` installs credential references as mode-`0600` server files,
and `wt-server` redacts all values and responses that may contain tokens.

For GitHub, use GitHub Apps rather than personal access tokens. Separate app
authority so the publisher has Contents write permission only on the staging
fork, while canonical access is limited to metadata and pull-request
operations. Mint installation tokens for the minimum repositories and
permissions for each action; installation tokens expire after one hour.

For GitLab, use dedicated project-scoped service identities and expiring
project access tokens. Separate staging-repository Git write authority from
canonical MR API authority where GitLab permissions allow. The identities are
never Maintainers or Owners of the canonical project, cannot push protected
branches, and are not eligible to approve or merge their own MRs.

Branch protection and fork-pipeline policy remain required on the external
forge. They are defense in depth for publisher defects, not a replacement for
the assignment checks.

### Failure and cleanup

Forgejo and external mutations use durable reconciliation keys. Interrupted
claims and publications resume from observed state and never infer success from
an API timeout.

Deleting a world first revokes its Forgejo token and stops or network-isolates
the VM. WT then deletes its Forgejo fork and technical identity. Unpublished
proxy branches are intentionally discarded by `wt rm`.

Published external staging refs and open PRs or MRs remain on GitHub or GitLab
after world deletion so review is not destroyed. They become detached WT
assignments. Reconciliation may delete a staging branch only after its PR or MR
is merged or closed and the configured retention period has elapsed.

If Forgejo or the external forge is unavailable, WT records cleanup or
publication as pending and retries. It never reports a commit as externally
published until the authoritative forge returns the exact assigned head.

### Component ownership

- `wt` creates and changes trusted assignments and reports proxy and external
  publication state.
- `wt-server` owns assignment validation, typed external-forge adapters,
  reconciliation, leases, exact-ref publication, and external cleanup.
- `wt-provider` owns the empty credential mount, claim-time Git configuration,
  agent-forwarding policy, and guest verification.
- `wt-guest` owns the injected `wt-pr` helper and token-safe Forgejo access.
- `wt-libvirt` remains unaware of forges, Git refs, and credentials.
- `wt-server-setup` installs and validates Forgejo, GitHub App, and GitLab
  integration configuration and credential files.
- Forgejo owns disposable proxy repositories, world identities, forks, and
  world tokens.
- GitHub or GitLab owns canonical refs, staging refs, PR or MR state, review,
  CI, branch protection, and merge.
- Real Forgejo, external-forge, and KVM tests belong in
  `wt-integration-tests`.

Forgejo, GitHub, and GitLab are external services. WT does not install, upgrade,
or back them up.

## Verification

- Two worlds receive different Forgejo identities, forks, and tokens.
- A world can create, rewrite, and delete arbitrary branches only in its own
  proxy fork.
- An unassigned proxy ref never appears on GitHub or GitLab.
- Each assignment can update only its exact external staging ref and PR or MR.
- Rewriting an assigned branch succeeds only with the recorded lease; an
  unexpected external update produces a conflict.
- An agent cannot obtain an external token, change its assignment, publish a
  tag or different ref, write a canonical branch, affect another assignment,
  approve, or merge.
- External pull or merge requests retain untrusted-fork CI behavior and cannot
  access protected secrets without the external forge's explicit approval.
- No workstation agent socket is available through guest or app SSH.
- No Forgejo token exists in a generation or unclaimed ready world.
- GitHub installation tokens are repository- and permission-limited and expire;
  GitLab credentials are project-scoped, expiring, and non-maintainer.
- Failed claims, timed-out pushes, and API retries do not duplicate external
  branches or PRs and do not advance a lease without observing success.
- World deletion revokes proxy access while preserving already-published
  external review state.
- Existing non-mediated worlds retain their current Git and SSH-agent behavior.

## Consequences

Agents get an ergonomic, ordinary Git remote where destructive branch
operations are low risk. The valuable GitHub or GitLab authority stays in a
typed server-side publisher that accepts capabilities from trusted assignments,
not destinations from agent input.

Forgejo is not another source of truth. Losing an unpublished proxy fork loses
that work; published commits and review state live on the external forge.

WT gains distributed transactional state across SQLite, libvirt, Forgejo, and
one external forge. Exact retries, leases, negative authorization tests, and
clear degraded states are required.

The WT server holds external publisher credentials. Host compromise can exceed
an individual assignment even though world compromise cannot. Splitting read,
staging-write, and PR or MR credentials, using short expirations, and retaining
external branch protection reduce but do not eliminate that host-level risk.

An agent may still generate malicious code and cause untrusted CI to be
requested. Review and external fork-pipeline rules remain the boundary before
that code receives protected CI secrets or reaches the canonical branch.

## Alternatives

### Give each agent a GitHub or GitLab token

Rejected because neither forge offers a token whose complete authority is one
branch and one PR or MR. Repository-scoped credentials would expose unrelated
refs and API operations.

### Make Forgejo canonical and mirror it outward

Rejected because existing work, review, CI, and merge authority must remain on
GitHub or GitLab. A push mirror would move the source of truth and create a
second review system.

### Let agents push directly to protected canonical repositories

Rejected because branch protection is policy on a broad credential, not a
narrow capability. The staging fork and publisher prevent the credential from
entering the world at all.

### Use one shared Forgejo fork and token for all agents

Rejected because one compromised world could overwrite another assignment's
proxy branch or destroy its unpublished work.

### Publish every branch from the per-world fork

Rejected because branch creation would implicitly create external authority.
Only trusted assignment records may add a publication mapping.

### Expose a generic external-forge API proxy

Rejected because filtering arbitrary API paths and request bodies is difficult
to audit. The publisher implements a small set of typed operations with stored
repository, ref, and PR or MR identities.

### Use only local Git repositories in the VM

Rejected because a destroyed or failed world would lose all unpublished work,
and reviewers would have no durable remote branch to inspect before external
publication.

## References

- [Forgejo access-token scopes](https://forgejo.org/docs/latest/user/token-scope/)
- [Forgejo repository mirrors](https://forgejo.org/docs/latest/user/repo-mirror/)
- [GitHub App installation authentication](https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/authenticating-as-a-github-app-installation)
- [GitHub App permission model](https://docs.github.com/en/apps/using-github-apps/authorizing-github-apps)
- [GitLab project access tokens](https://docs.gitlab.com/user/project/settings/project_access_tokens/)
- [GitLab merge requests API](https://docs.gitlab.com/api/merge_requests/)
