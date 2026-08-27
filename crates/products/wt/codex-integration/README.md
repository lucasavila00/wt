# wt-codex-integration

`wt-codex-integration` synchronizes Codex's local history from the shared
rollout files before Codex starts.

```text
wt-codex-integration reconcile
```

The golden image owns two direct links to the integration executable:
`~/.local/bin/codex` and `/usr/local/bin/codex`. Before starting the real CLI,
either link synchronizes local history and prints its progress to standard output. The
integration asks Codex's app server to perform its documented rollout scan and index repair, then
executes the upstream CLI at
`~/.codex/packages/standalone/current/bin/codex`. Reconciliation failure stops
Codex from starting. `IGNORE_CODEX_WT_CHECKS=true` bypasses the refresh and starts Codex
immediately.

The image recipe installs and verifies the links. Per-world provisioning does
not replace or repair the image-owned entrypoints. Remove a world with damaged
Codex integration and create it again from a verified guest image.
