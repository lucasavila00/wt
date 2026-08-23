# ADR 0075: Centralize holistic Git state

- Status: Proposed
- Date: 2026-08-23

## Decision

Keep checkout state, Git gateway activity, and `wt-tools` provider API activity
as separate facts. Reuse the configured `(provider_host, repository)` key from
Git activity and add a derived read model that joins facts when a product needs
one repository view.

Checkout observations remain keyed by world, session, and cwd. They associate
with the repository key only when their selected remote resolves unambiguously
to a configured provider and repository. Gateway and provider activity retain
their own operation time and provenance. Do not derive one source from another
or let any Git update change Codex lifecycle state or age.

## Consequences

The read model is not a new authority or mutable state owner. WT can present
repository history holistically without implying that a remote operation or PR
target is the active checkout. Unconfigured, local-only, unsupported, and
ambiguous remotes remain visible checkout state but unjoined. SSH and HTTPS
aliases remain distinct until an explicit mapping policy says otherwise. A
combined view labels lifecycle, Git-check, and operation timestamps separately.
