# Codex config

Copy the sample into a repository that runs in WT worlds:

```bash
mkdir -p .codex
cp examples/codex-config/config.toml .codex/config.toml
```

The session-start hook asks `wt-git-hosting` for the current coding-agent instructions.
The gateway owns the full prompt, including its writable branch prefix, so it can
update the guidance without changing this file. The hook stays silent outside a
running WT environment. Review and trust the project hook in Codex before using
it.
