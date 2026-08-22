# wt-codex-integration

`wt-codex-integration` makes shared Codex rollout files visible to Codex's local session
index.

```text
wt-codex-integration reconcile
wt-codex-integration install-config
```

The golden image owns two direct links to the integration executable:
`~/.local/bin/codex` and `/usr/local/bin/codex`. The integration asks Codex's
app server to perform its documented rollout scan and index repair, then
executes the upstream CLI at
`~/.codex/packages/standalone/current/bin/codex`. Reconciliation failure emits
a warning but does not prevent Codex from starting.

`install-config` installs WT's exact environment configuration at
`$CODEX_HOME/config.toml`. It fails if a different configuration already
exists.

The image recipe installs and verifies the links. Per-world provisioning does
not replace or repair the image-owned entrypoints. Remove a world with damaged
Codex integration and create it again from a verified retained image.
