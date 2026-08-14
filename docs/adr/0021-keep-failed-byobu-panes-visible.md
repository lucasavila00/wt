# ADR 0021: Keep failed Byobu panes visible

- Status: Accepted
- Date: 2026-08-13

## Context

Creating a pane in `mt2` appeared to do nothing. Tmux created it, but the
devcontainer login failed and the pane disappeared with the error.

```text
bash: /home/vscode/.local/bin/ghostty-dev-shell: No such file or directory
```

## Decision

WT always sets tmux `remain-on-exit failed`. Remove the code that changes it to
`off` after world setup.

`wt-app-pane` waits for the devcontainer SSH command. On failure, it preserves
the original error and prints:

```text
wt: could not open the devcontainer shell (exit 127)
wt: fix the error above, close this pane, and create a new one
```

## Consequences

Failed panes remain visible with a useful error. Successful logouts still close
their panes. WT does not fall back to a guest shell because it is the wrong
development environment.
