# ADR 0007: Tune managed SSH connections

- Status: Accepted
- Date: 2026-07-31

Every managed alias uses a 30-second keepalive, three missed keepalives, and
key-only authentication. WT does not set `BatchMode`, so OpenSSH may still ask
to unlock a private key.

Remote contexts enable compression and local contexts disable it. Both the
Byobu alias and the `-direct` alias follow the context setting. Complete local
and remote configurations are snapshot-tested.
