# wt-codex-integration

The golden image owns two direct links to the integration executable:
`~/.local/bin/codex` and `/usr/local/bin/codex`. Either link immediately
executes the upstream CLI at `~/.codex/packages/standalone/current/bin/codex`.
Each world mounts only its own server-backed sessions directory, so Codex sees
only that world's history.

The image recipe installs and verifies the links. Per-world provisioning does
not replace or repair the image-owned entrypoints. Remove a world with damaged
Codex integration and create it again from a verified guest image.
