# ADR 0017: Prewarm project branches with ready-world pools

- Status: Accepted
- Date: 2026-08-09

## Context

Creating a world from the golden image still clones the repository, builds or
pulls its development images, creates Docker resources, and runs the
devcontainer lifecycle. Those project-specific steps dominate startup even
though many worlds may use the same project branch.

For example, a developer may repeatedly create worlds from the `main` branch
of [Diffo](https://github.com/lucasavila00/diffo). The worlds need separate
disks, Docker daemons, SSH identities, and writable checkouts, but most of their
initial disk state is identical.

ADR 0015 provides a useful primitive: a running world can be forked from an
immutable disk point, and the fork receives new machine and SSH identities.
A fork still has to boot and restart its containers, however. It is much faster
than a new world but is not an immediate allocation.

The updater also cannot depend on the workstation's forwarded SSH agent. The
agent exists only during an interactive connection, and WT deliberately does
not retain Git private keys on the server or in guests.

## Decision

Add owner-scoped prewarm policies for project branches. A policy contains:

- a policy name;
- the SSH Git source that claimed worlds use as `origin`;
- an unauthenticated, read-only fetch URL for background refreshes;
- one branch;
- the CPU, memory, and disk profile;
- the Git author and authorized public keys gathered in the same way as
  `wt new`;
- a refresh interval and desired ready-world count.

The first implementation supports only fetch URLs that need no credentials.
For a public GitHub repository, the fetch URL may be its HTTPS clone URL while
the claimed world's origin remains its SSH URL. Private-repository prewarming
requires a separate credential design and is not part of this decision.

`wt prewarm add`, `wt prewarm ls`, and `wt prewarm rm` manage policies through
the existing authenticated control plane. Policies and their resources belong
to the calling owner. They are stored in the server registry, but managed
generations and unclaimed worlds are not shown by `wt ls` and cannot be used as
ordinary worlds.

### Immutable generations

The server runs a reconciliation loop as part of its existing world-lifecycle
coordination. For each policy it checks the configured branch at the refresh
interval and records the exact observed commit.

When the commit differs from the active generation, the server creates a clean
candidate from the golden image and provisions it through `wt-provider` using
the policy's read-only fetch URL. It checks out the branch at the observed
commit, applies the policy's non-secret world inputs, starts the stock
devcontainer, runs its normal lifecycle, and verifies guest and app SSH. A
candidate becomes the active generation only after all verification succeeds.

Generations are never refreshed in place. A failed candidate leaves the
previous generation and claimed worlds unchanged. If the branch advances
again during a build, reconciliation starts another candidate after the
current attempt finishes.

The active generation is the copy-on-write source for the policy. It retains
the complete prepared guest disk: checkout, local Docker images, build cache,
volumes, devcontainer, and lifecycle results. It contains public access
material but no Git private key, agent socket, token, or other user secret.

### Ready-world pool

The reconciler maintains the configured number of ready worlds by disk-only
forking the active generation. Each ready world is booted, receives unique
machine, guest SSH, app SSH, and session identities, restarts its containers,
and passes the existing readiness checks before entering the pool. No ready
world has been exposed to a user.

A matching `wt new` request atomically claims one ready world. Before publishing
it, the server:

1. assigns the requested world name and normal registry ownership;
2. verifies that the request's source, branch, resources, Git author, and
   authorized keys exactly match the policy;
3. changes the checkout's `origin` from the read-only fetch URL to the policy's
   SSH source; and
4. verifies the guest and app endpoints one final time.

Claiming performs no guest boot, Git fetch, image build, devcontainer startup,
or lifecycle command. The claimed world is then an ordinary independent world:
it appears in `wt ls`, may be forked or removed, and is never changed by the
prewarm reconciler.

If the pool is empty but an active generation is current, `wt new` synchronously
forks that generation. This retains the project-specific speedup but includes
guest boot and container restart latency. If no current verified generation
exists, WT uses the normal creation path.

The pool is replenished asynchronously after claims. When a new generation is
promoted, old unclaimed worlds stop being eligible immediately and are removed.
Claimed worlds continue to reference their existing disk graph normally.

### Freshness and failure

A generation represents the exact commit last successfully observed by the
reconciler, not a moving branch name. The commit and observation time are
stored and exposed by `wt prewarm ls` and creation progress.

WT does not knowingly claim a stale ready world. Once the reconciler observes a
new commit, old ready worlds are ineligible while the replacement generation
builds. A failed build, an overdue successful remote check, or a failed ready
world causes matching requests to use normal creation instead of silently
returning an old branch revision.

Remote changes immediately after a successful poll remain an unavoidable
bounded race. Healthy policies may lag the upstream branch by at most their
configured refresh interval, and WT reports the exact claimed commit.

Reconciliation state, generations, ready worlds, and disk references are
durable registry data. Server restart resumes reconciliation and garbage
collection rather than relying on in-memory jobs.

### Component ownership

- `wt` owns the prewarm commands, gathers normal non-secret user inputs, and
  reports the selected commit.
- `wt-server` owns policy persistence, reconciliation, transactional claims,
  generation state, and disk-graph references.
- `wt-provider` owns candidate provisioning, claim-time guest customization,
  and readiness decisions.
- `wt-libvirt` continues to own copy-on-write machine forks and identity
  replacement; it does not learn about Git or prewarm policies.
- `wt-server-setup` owns any installed service configuration needed to enable
  the reconciler. No separate host-side prewarm daemon or build tool is added.

## Verification

- A candidate is not promoted until its exact commit, devcontainer, guest SSH,
  and app SSH have been verified.
- Claiming a ready world runs no boot, fetch, build, container-start, or
  devcontainer lifecycle operation.
- Concurrent creates cannot claim the same ready world.
- Claimed worlds have distinct machine, guest SSH, app SSH, and session
  identities and isolated writable disk heads.
- Updating a policy never changes an already claimed world.
- A failed refresh preserves the last verified generation but does not serve it
  as the newly observed branch head.
- No private Git credential or live agent socket is present in a generation or
  ready world.
- Deleting policies, generations, ready worlds, and claimed worlds preserves
  reachable disk nodes and garbage-collects unreachable nodes.
- Real KVM tests show that a ready-world claim is materially faster than a
  synchronous fork and that a synchronous generation fork is materially faster
  than normal creation.

## Consequences

The common path can allocate a fully running project world without repeating
project setup or waiting for a VM to boot. Bursts beyond the ready count still
reuse all prepared project disk state.

Each policy consumes one active generation plus its configured number of
running ready VMs. Disk use is copy-on-write, but ready VMs reserve host memory
and CPU capacity. Pool size is therefore an explicit operational choice rather
than an automatic unbounded cache.

Configuring a policy authorizes WT to fetch and execute new code from that
branch automatically. Repository and devcontainer code already run inside a
world's trust boundary, but prewarming moves that execution from a user-started
operation to a background operation.

Prewarming is exact only for requests that match the policy's complete initial
state. Different revisions, resources, Git authors, or authorized keys follow
the normal creation path and do not cause WT to create speculative pools.

Private repositories are not prewarmed initially. Adding a deploy key to a
generation would copy it into every descendant disk and violate WT's credential
model. A future host-side mirror or credential broker needs its own trust and
lifecycle decision.

## Alternatives

### Export a devcontainer or OCI image

Rejected because an image does not contain the prepared guest, checkout,
Docker volumes and networks, lifecycle results, WT helpers, or app SSH setup.
Reconstructing those pieces would retain much of the current startup time.

### Keep only a stopped disk image

Rejected as the only cache level because it still puts VM boot, container
restart, and endpoint verification on every create. The active generation
provides this fallback, while the bounded running pool supplies the immediate
path.

### Snapshot VM memory

Rejected because it would duplicate live processes, network state, random
state, connections, and credentials. WT keeps disk-only forks and starts every
machine identity independently.

### Pull and rebuild one long-lived template in place

Rejected because consumers could observe a partially updated checkout or
Docker state, and a failed update could destroy the last usable template.
Immutable candidate generations make promotion atomic.

### Refresh every ready world independently

Rejected because it repeats Git and devcontainer work, creates inconsistent
pool members during updates, and makes branch movement race with allocation.
All ready worlds instead descend from one verified exact-commit generation.

### Store a private deploy key in the template

Rejected because copy-on-write isolation does not erase inherited disk data.
Every claimed world could recover the key and use it outside the intended
background fetch operation.
