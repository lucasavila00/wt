# ADR 0033: Bootstrap managed SSH configuration

- Status: Proposed
- Date: 2026-08-15
- Amends: [ADR 0011](0011-exec-ssh-after-world-creation.md)

## Context

`wt sync` writes aliases to `~/.ssh/wt/config`, but OpenSSH reads them only
when `~/.ssh/config` includes that file. Without the include, `wt new` execs
`ssh CONTEXT.NAME` and OpenSSH incorrectly attempts DNS resolution.

## Decision

`wt sync` ensures `~/.ssh/config` contains this global include before its
`Host` and `Match` blocks:

```sshconfig
Include ~/.ssh/wt/config
```

Create `~/.ssh/config` when absent. Otherwise preserve its other includes,
contents, ordering, and permissions; add no duplicate; and update it atomically.

If WT cannot safely ensure the include, synchronization fails with the cause,
the directive, and its required placement. `wt new` reports that the world was
created, does not exec SSH, and instructs the user to add the directive, run
`wt sync`, and reconnect with the qualified alias.

## Verification

- Resolve local and remote aliases with real OpenSSH configuration evaluation.
- Verify creation, preservation, permissions, atomicity, and idempotence.
- Snapshot the update-failure diagnostic and verify `wt new` does not exec SSH.
