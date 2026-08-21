# wt-codex-integration

`wt-codex-integration` makes shared Codex rollout files visible to Codex's local session
index.

```text
wt-codex-integration reconcile
wt-codex-integration install
wt-codex-integration uninstall
wt-codex-integration remove
```

`install` replaces the `codex` command found in `PATH` with a trampoline. The
trampoline reconciles sessions, then runs the saved Codex CLI. `uninstall` and
its `remove` alias restore the saved command.
