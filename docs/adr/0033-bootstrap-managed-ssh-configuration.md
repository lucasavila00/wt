# ADR 0033: Bootstrap managed SSH configuration

- Status: Accepted
- Date: 2026-08-15
- Amends: [ADR 0011](0011-exec-ssh-after-world-creation.md)

## Context

`wt sync` writes aliases to `~/.ssh/wt/config`, but OpenSSH reads them only
when `~/.ssh/config` includes that file. Without the include, `wt new` execs
`ssh CONTEXT.NAME` and OpenSSH incorrectly attempts DNS resolution.

## Decision

When `~/.ssh/config` is absent, `wt sync` creates it with:

```sshconfig
Include ~/.ssh/wt/config
```

Publish the new file atomically with mode `0600` and without replacing a file
created concurrently. Never modify an existing `~/.ssh/config`; require it to
contain the WT include as a global directive outside `Host` and `Match` blocks.

Other global directives and includes may precede it. If the existing
configuration does not load WT globally, synchronization fails with the
directive and its required placement. `wt new` reports that the world was
created, does not exec SSH, and instructs the user to update the file, run `wt
sync`, and reconnect with the qualified alias.

## Verification

- Resolve local and remote aliases with real OpenSSH configuration evaluation.
- Verify no-clobber creation, mode `0600`, existing-file preservation, and
  idempotence.
- Snapshot the update-failure diagnostic and verify `wt new` does not exec SSH.
