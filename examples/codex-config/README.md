# Codex config

Copy the sample into a repository that runs in WT worlds:

```bash
mkdir -p .codex
cp examples/codex-config/config.toml .codex/config.toml
```

The session-start hook tells Codex when it is inside the devcontainer and
explains the shared `wt/` Git namespace. Review and trust the project hook in
Codex before using it.
