# ADR 0046: Share Codex authentication with retained worlds

- Status: Accepted
- Date: 2026-08-21
- Amends: [ADR 0042](0042-share-configured-folders-with-retained-worlds.md)

## Context

We already share Codex sessions between the server, worlds, and the
devcontainer. Users still have to log in separately because Codex auth lives
in `.codex/auth.json`.

## Decision

The KVM host must already be logged in to Codex. Share its live `auth.json`
with worlds alongside the existing sessions folder:

```toml
[[shared_files]]
source = "/home/wt/.codex/auth.json"
target = ".codex/auth.json"
```

Add this to the development and KVM server examples, and document it in the
server-config README. This is a shared-file mount, not a copy: worlds see
updates to the host credential but cannot write it back. Test fixtures should
use a disposable credential or leave the mount out when authentication is not
under test.

The repository devcontainer mirrors the session mount:

```yaml
volumes:
  - /home/wt/.codex/sessions:/home/wt/.codex/sessions
  - /home/wt/.codex/auth.json:/home/wt/.codex/auth.json:ro
```

The shared file is mounted read-only in both the KVM world and its
devcontainer. Do not share the complete `.codex` directory:
Codex indexes, databases, logs, and locks remain local to each world.

## Implementation constraints

Before implementation, resolve the following:

- WT currently mounts directories; define the regular-file mount contract and
  make sure host-side atomic replacement of `auth.json` is visible in worlds
  and nested devcontainers.
- Require file-backed Codex auth and a regular, non-symlink `auth.json`; do not
  assume that a logged-in Codex account always uses this file.
- Define what happens when a token expires. Refresh must be performed by the
  logged-in WT server `wt` user, while worlds remain read-only consumers.
- Keep the devcontainer bind mount repository-controlled, as with sessions.
- Treat test credentials as disposable, and define whether existing worlds are
  recreated or explicitly migrated after the configuration changes.

## Consequences

New worlds and the devcontainer reuse the host's login. The credential remains
outside world disks, but any trusted process in a world with the mount can use
it, so read-only access does not prevent exfiltration or account-wide impact.
Re-login or credential refresh happens on the WT server host.
