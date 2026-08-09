# ADR 0019: Prewarm project branches

- Status: Deferred
- Date: 2026-08-09

## Context

Most of `wt new` is project setup: fetching Git, building images, creating
Docker resources, and running the devcontainer lifecycle.

ADR 0015 can fork prepared disk state, but the fork still has to boot and
restart its containers. This optimization should wait until ADRs 0017 and 0018
work end to end with normal world creation.

## Decision

Later, add owner-scoped prewarm policies for a project branch, VM profile,
refresh interval, and ready-pool size.

A policy uses an unattended read source. It must resolve the branch to an exact
commit without leaving a credential in the world. Public HTTPS is one option;
the Forgejo mirror from ADR 0017 is another.

A generation is one clean, fully prepared commit. The first starts from the
golden image. Later generations fork the active one so Docker and BuildKit
caches survive. A candidate becomes active only after its checkout,
devcontainer, and SSH endpoints pass. A failed candidate leaves the active
generation alone.

WT keeps a bounded pool of running forks of the active generation. Every ready
world has its own writable disk and machine and SSH identities, but no user
keys, Git author, token, assignment, or forwarded agent socket.

`wt new` atomically reserves one ready world, adds the claimant's identity and
Git configuration, verifies it, and then makes it reachable. If the pool is
empty, WT boots a new fork of the generation. If no current generation exists,
it uses normal creation.

Every generation records its exact commit and observation time. Once WT sees a
newer commit, older ready worlds are no longer eligible. Failed or overdue
refreshes fall back to normal creation rather than knowingly serving stale
state.

Claimed worlds are independent and are never refreshed by the pool. ADR 0020
defines when prepared Docker state can be reused between generations.

## Verification

- Only a verified exact-commit candidate can become active.
- Concurrent creates cannot claim the same world.
- Shared state contains no claimant or Git credential.
- A newly observed commit immediately retires older ready worlds.

## Consequences

The common path returns a running world. Overflow pays for a boot, but not
project setup. Pools use bounded memory and CPU even when idle.

Prewarming executes new repository code without a user present. Creating a
policy explicitly grants that authority inside WT's VM boundary.

## Alternatives

An OCI image omits the prepared guest, checkout, volumes, and lifecycle state.
A stopped disk still boots on every request. Updating one shared template in
place could expose a failed refresh.
