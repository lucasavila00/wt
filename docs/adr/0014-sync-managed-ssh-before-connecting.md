# ADR 0014: Sync managed SSH before connecting

- Status: Accepted
- Date: 2026-08-16

`wt ssh NAME` resolves a short or qualified world from the complete inventory,
synchronizes managed SSH state, then replaces itself with
`ssh -- CONTEXT.WORLD`. It does not start OpenSSH when inventory collection or
synchronization fails.

The regular alias opens Byobu. Direct command access remains available through
OpenSSH as `CONTEXT.WORLD-direct`.
