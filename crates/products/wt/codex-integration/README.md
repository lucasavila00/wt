# wt-codex-integration

`wt-codex-integration` makes shared Codex rollout files visible to Codex's local session
index.

```text
wt-codex-integration reconcile
wt-codex-integration install
wt-codex-integration install-config
wt-codex-integration uninstall
```

`install` writes WT's exact environment configuration to
`$CODEX_HOME/config.toml` and replaces the `codex` command found in `PATH` with
a trampoline. It fails if a different configuration already exists. The
trampoline reconciles sessions, then runs the saved Codex CLI. `uninstall`
restores the saved command.

`install-config` installs only the user configuration, for environments where
the trampoline is already provided by the host.
