# ADR 0051: Make wt-tools targets explicit

- Status: Accepted; Date: 2026-08-21

## Decision

Every provider operation requires a `target` and `command`; feedback operations omit `target`.

```text
wt-tools '{"target":{"provider":"github","repository":"acme/widget"},"command":{"action":"show_mr","mr":"7"}}'
wt-tools '{"target":{"provider":"gitlab","repository":"acme/widget"},"command":{"action":"list_ci","commit":"abc1234"}}'
```

`wt-tools` will stop reading `remote.origin.url`. The gateway will validate the
repository and map `provider` only to its installer-configured host, endpoint, and
credential. Requests cannot supply connection or credential values.
## Consequences

Commands work from any directory. Callers repeat the target in every command;
credential access and the `wt/` Git branch write policy do not change.
