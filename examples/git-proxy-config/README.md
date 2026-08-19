# Git proxy config

Install the standalone Git proxy with:

```text
scripts/install-git-server --config examples/git-proxy-config/wt-git-proxy.development.toml
```

The provider key must already have the intended GitHub repository access.
Setup installs a protected runtime copy; it does not generate or change the
source key. Keep this install input for later upgrades or policy changes.
