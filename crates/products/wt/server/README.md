# wt-server

Owner-scoped control-plane daemon for one KVM server.

Systemd runs `wt-server serve` as the installed server user. It listens only on
the mode-`0600` Unix socket `/run/wt/server.sock`. `wt` invokes `wt-server api`
locally or through OpenSSH; that command bridges one protocol version 6 JSON
request, line-delimited progress events, and a final response between stdio and
the daemon.

## Owns

- Create, list, get, start, stop, and delete operations for retained worlds.
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

Install: [Development and setup](../../DEVELOPMENT.md). System flow:
[Architecture](../../docs/internals/architecture.md).
