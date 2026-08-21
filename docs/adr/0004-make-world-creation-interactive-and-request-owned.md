# ADR 0004: Make world creation interactive and request-owned

- Status: Accepted
- Date: 2026-07-15

## Decision

`wt new` uses one interactive form for context, name, CPU, RAM, disk, discovered
public SSH keys, Git author, and confirmation. The client validates every value
and sends the complete typed request only after confirmation.

The server configuration owns infrastructure and policy, not per-world sizes
or workstation keys. The server fingerprints the complete request so an exact
retry can observe an in-flight or completed creation while a different request
conflicts.

The client reads regular `~/.ssh/*.pub` files, validates and deduplicates them,
and never reads private keys or agent contents. Creation requires a terminal;
cancellation before confirmation leaves no state. A failed provisioning run is
removed and recreated from the golden image rather than resumed.

## Consequences

Different worlds may use different resources and the requesting workstation
controls guest access. Non-interactive creation is intentionally unsupported.
