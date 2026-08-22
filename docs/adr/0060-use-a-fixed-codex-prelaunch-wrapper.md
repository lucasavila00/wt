# ADR 0060: Use a fixed Codex prelaunch wrapper

- Status: Accepted; Date: 2026-08-22
- Amends: [ADR 0046](0046-share-codex-auth-with-retained-worlds.md), [ADR 0059](0059-bake-static-guest-binaries-into-golden-images.md)

## Problem

Codex must discover rollout files from WT's shared sessions mount before it
renders a resume picker. A `SessionStart` hook runs after a session is chosen,
so it cannot provide that guarantee.

The first prelaunch implementation renamed the vendor-owned
`~/.local/bin/codex` link to a hidden sibling, installed a trampoline in its
place, and inferred the saved CLI from the invoked path. The golden image also
provided `/usr/local/bin/codex` as an outer alias. Launching through that alias
made the trampoline look for the saved CLI in `/usr/local/bin` instead of the
user directory. Supporting arbitrary alias chains would add machinery around a
topology that WT already controls.

The implementation also duplicated Codex behavior by scanning and parsing
rollout files, listing only state-database threads, and using `thread/read` for
an undocumented indexing side effect.

## Decision

Keep a prelaunch wrapper, but make the golden-image contract fixed and small:

```text
/home/wt/.local/bin/codex -> /usr/local/bin/wt-codex-integration
/usr/local/bin/codex      -> /usr/local/bin/wt-codex-integration
real CLI                  = /home/wt/.codex/packages/standalone/current/bin/codex
```

Both shell PATH orders therefore enter the same wrapper. The wrapper validates
the fixed upstream executable, starts its app server, and calls `thread/list`
without `useStateDbOnly`. Codex documents that default operation as scanning
rollout logs and repairing their state metadata. The wrapper then uses `exec`
to preserve arguments, environment, working directory, standard streams,
signals, process identity, umask, and exit status.

Index refresh is best-effort: a failure prints a warning and still starts the
real CLI. A missing or non-executable upstream CLI is a hard image-contract
error that tells the operator to recreate the world from a verified image.

The image recipe owns the two links and verifies both by executing `--version`.
The runtime has no install or uninstall command, performs no PATH discovery or
alias traversal, keeps no `.codex.wt-real` sibling, and does not recursively
rewrite shared-session permissions during launch.

## Consequences

- Resume-picker freshness still occurs at the only boundary early enough to
  affect initial UI state.
- Codex remains launchable when session-index repair is unavailable.
- The integration depends only on documented Codex app-server behavior instead
  of maintaining a second rollout parser.
- Codex updater compatibility depends on the stable standalone `current` path.
  Image finalization and KVM tests must detect a changed upstream layout.
- Worlds created from the earlier topology should be recreated; no migration
  path is required during early testing.
