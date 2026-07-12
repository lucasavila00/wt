# wt-server

Owner-scoped control-plane helper for one KVM server.

`wt` invokes `wt-server api` locally or through OpenSSH. One protocol version 1
JSON request enters on stdin; one response leaves on stdout. The OS user running
the helper owns the request.

## Owns

- Create, list, get, delete, and logs operations.
- SQLite world registry and provisioning logs.
- World locks and detached provisioning jobs.
- Reconciliation after worker failure.
- Dispatch to `wt-libvirt`.

It does not listen on a socket, manage SSH authentication, or implement KVM
lifecycle.

## State

| Path | Contents |
|------|----------|
| `/etc/wt/server.toml` | Strict runtime configuration |
| `~/.local/state/wt/instances.db` | User registry and logs |
| `~/.local/state/wt/jobs` | Per-world OS locks |

Accepted provisioning jobs survive client disconnects. A worker crash becomes
`error` on the next API operation; cleanup requires `wt rm`.

## Smoke test

```text
printf '%s\n' '{"protocol_version":1,"operation":"list"}' | wt-server api
```

Install: [Getting started](../../GETTING-STARTED.md). System flow:
[How WT works](../../docs/how/README.md).
