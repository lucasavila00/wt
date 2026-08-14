# wt-server

Owner-scoped control-plane daemon for one KVM server.

Systemd runs `wt-server serve` as the installed server user. It listens only on
the mode-`0600` Unix socket `/run/wt/server.sock`. `wt` invokes `wt-server api`
locally or through OpenSSH; that command bridges one protocol version 1 JSON
request and response between stdio and the daemon.

## Owns

- Create, list, get, start, and delete operations for retained worlds.
- Typed dispatch to devcontainer and host lifecycles.
- Shared SQLite guest, capacity, and copy-on-write disk-graph registry.
- In-memory coordination of concurrent world operations.
- Reconciliation after worker failure.
- Rejection of GitHub CI worlds, whose operator service is not shipped yet.

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

Install: [Getting started](../../GETTING-STARTED.md). System flow:
[Architecture](../../docs/internals/architecture.md).
