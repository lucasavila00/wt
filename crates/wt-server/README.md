# wt-server

Owner-scoped control-plane daemon for one KVM server.

Systemd runs `wt-server serve` as the installed server user. It listens only on
the mode-`0600` Unix socket `/run/wt/server.sock`. `wt` invokes `wt-server api`
locally or through OpenSSH; that command bridges one protocol version 1 JSON
request and response between stdio and the daemon.

## Owns

- Create, list, get, delete, and logs operations.
- SQLite world registry and provisioning logs.
- In-memory coordination of concurrent world operations.
- Reconciliation after worker failure.
- Dispatch to `wt-libvirt`.

It does not listen on TCP, manage SSH authentication, or implement KVM lifecycle.

## State

| Path | Contents |
|------|----------|
| `/etc/wt/server.toml` | Strict runtime configuration |
| `~/.local/state/wt/instances.db` | User registry and logs |
Accepted provisioning operations survive client disconnects. A daemon crash or
restart marks interrupted operations `error` at startup; cleanup requires
`wt rm`.

## Smoke test

```text
printf '%s\n' '{"protocol_version":1,"operation":"list"}' | wt-server api
```

Install: [Getting started](../../GETTING-STARTED.md). System flow:
[How WT works](../../docs/how/README.md).
