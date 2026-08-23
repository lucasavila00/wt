# wt-server

Owner-scoped control-plane daemon for one KVM server.

Systemd runs `wt-server serve` as the installed server user. It listens only on
the mode-`0600` Unix socket `/run/wt/server.sock`. `wt` invokes `wt-server api`
locally or through OpenSSH; that command bridges one JSON request, zero or more
line-delimited progress events, and exactly one final response between stdio
and the daemon. Progress delivery is best-effort and a disconnected observer
does not cancel world provisioning.

## Owns

- Create, list, get, start, stop, and delete operations for guests.
- Host lifecycle dispatch.
- SQLite world, capacity, and disk registry.
- In-memory coordination of concurrent world operations.
- Reconciliation after worker failure.

It does not listen on TCP, manage SSH authentication, or implement KVM lifecycle.

## State

| Path | Contents |
|------|----------|
| `/etc/wt/server.toml` | Strict runtime configuration |
| `/etc/wt/capacity.toml` | Shared CPU, RAM, and disk limits |
| `~/.local/state/wt/instances.db` | User registry |
Accepted provisioning operations survive client disconnects. A daemon crash or
restart marks interrupted operations `error` at startup; cleanup requires
`wt rm`.

## Smoke test

```text
wt ls
```
