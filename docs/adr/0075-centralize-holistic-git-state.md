# ADR 0075: Centralize holistic Git state

- Status: Proposed
- Date: 2026-08-23

## Decision

Keep checkout state, Git gateway activity, and `wt-tools` provider activity as
separate facts. Add a canonical repository key and a read model that can join
those facts when a product needs one repository view.

Codex checkout observations attach their cwd, branch, and Git freshness to the
key. Gateway and provider activity attach their operation time and provenance
to the same key. Do not derive one source from another or let any Git update
change Codex lifecycle state or age.

## Consequences

WT can present repository history holistically without implying that a remote
operation or PR target is the active checkout. SSH and HTTPS remotes require a
deliberate canonicalization policy; ambiguous or unconfigured remotes stay
unjoined. A combined view labels lifecycle, Git-check, and operation timestamps
separately.
