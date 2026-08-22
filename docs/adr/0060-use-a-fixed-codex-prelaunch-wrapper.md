# ADR 0060: Use a fixed Codex prelaunch wrapper

- Status: Accepted; Date: 2026-08-22
- Amends: [ADR 0046](0046-share-codex-auth-with-retained-worlds.md), [ADR 0059](0059-bake-static-guest-binaries-into-golden-images.md)

## Context

Codex must index rollout files from WT's shared sessions mount before rendering
its resume picker. A prelaunch boundary is required because session hooks run
after a session is selected or created.

## Decision

The retained golden image owns this fixed command topology:

```text
/home/wt/.local/bin/codex -> /usr/local/bin/wt-codex-integration
/usr/local/bin/codex      -> /usr/local/bin/wt-codex-integration
real CLI                  = /home/wt/.codex/packages/standalone/current/bin/codex
```

Both supported PATH orders enter the same wrapper. Before every launch, the
wrapper validates the upstream executable, starts its app server, discovers
shared rollout IDs, reads any missing threads through the app server, and
verifies that every shared session is indexed before it starts Codex.

Index refresh failure appends the complete diagnostic to
`~/.local/state/wt/codex-reconciliation.log` and prevents the real CLI from
starting. Setting `IGNORE_CODEX_WT_CHECKS=true` bypasses reconciliation when an
operator needs to start Codex without WT's session guarantees. A missing or
non-executable upstream CLI is an image contract failure and tells the operator
to recreate the world from a verified image.

The wrapper uses `exec` so the real CLI inherits the original arguments,
environment, working directory, standard streams, signals, process identity,
umask, and exit behavior.

The image recipe creates both command links and validates both with
`codex --version`. Per-world provisioning does not modify this topology.

## Consequences

- Shared sessions are visible when the initial Codex UI opens.
- Session-index failures block Codex unless the operator explicitly bypasses
  WT's checks.
- Codex owns rollout discovery and state repair through its documented app
  server behavior.
- The stable standalone `current` path is part of the image contract. Image
  finalization and KVM lifecycle tests detect an incompatible upstream layout.
