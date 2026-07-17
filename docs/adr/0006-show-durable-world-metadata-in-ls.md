# ADR 0006: Show durable world metadata in `wt ls`

- Status: Accepted
- Date: 2026-07-17

## Context

`wt ls` shows guest IP addresses and raw SSH endpoints. These values are useful
to the transport implementation, but users connect through WT's managed SSH
aliases. They are especially unhelpful for remote contexts, where the guest
address is reached through the context server rather than directly.

The create request also contains the requested CPU, RAM, disk, and initial Git
revision. Resources remain properties of the world. The revision does not: it
only selects the initial checkout, and the repository can move to another
branch or commit during normal development.

## Decision

Make the default `wt ls` columns `CONTEXT`, `NAME`, `STATUS`, `REPO`,
`RESOURCES`, and `DETAIL`.

- Derive `REPO` from the validated Git source and omit the full source URL.
- Persist and return the requested CPU, RAM, and disk values as instance
  metadata. Display them in one compact resources column.
- Keep lifecycle failures in `DETAIL`.
- Do not show guest IP addresses or raw SSH endpoints. Keep them in the API for
  managed SSH inventory generation.
- Do not persist or show the requested revision as current repository state.
  The create request and retry fingerprint retain it as an initial checkout
  input.

If `wt ls` shows a branch or commit in the future, obtain it on demand from the
running world and label failures or unavailable worlds explicitly. Do not infer
live Git state from creation inputs.

WT has not shipped. Replace the registry schema and protocol version 1 instance
shape in place without a migration or compatibility path.

## Consequences

- Local and remote contexts have the same useful default columns.
- The table reports configured capacity, not live utilization.
- The table does not present stale checkout information as current state.
- Existing development registries must be cleared and recreated.

## Alternatives

### Show the initial revision

Rejected. It becomes stale as soon as the checkout changes and would be
misleading under a `REVISION` heading.

### Query live Git state on every list

Deferred. It adds guest connections, latency, and partial-failure behavior to a
control-plane inventory command. It should be designed separately if the live
state is valuable enough to justify that cost.

### Keep IP and SSH columns

Rejected. Managed aliases are the user-facing connection interface, and raw
remote guest addresses are not directly actionable.
