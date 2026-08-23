# ADR 0005: Exec SSH after world creation

- Status: Accepted
- Date: 2026-07-22

## Context

World creation returns after the host is running and its SSH endpoint is ready.
The interactive creation flow should enter the persistent terminal workspace
without requiring a second command.

The SSH connection owns the interactive Byobu workspace, which survives later
disconnects.

## Decision

After a successful create response, `wt new` synchronizes the managed SSH
inventory and replaces its process with:

```text
ssh CONTEXT.NAME
```

Use the qualified alias because the context and world are already known and it
cannot become ambiguous when more contexts are configured.

Flush the creation summary before replacing the process. If starting OpenSSH
fails, report that failure as a `wt new` error. Once replacement succeeds,
OpenSSH owns the terminal, signal handling, and final process exit status.

Do not spawn OpenSSH as a child and wait for it. `wt` has no remaining work
after SSH starts, and retaining a wrapper would add signal and exit-status
handling without providing lifecycle value.

## Verification

- Create a world with a stub OpenSSH executable and verify it receives exactly
  the qualified `CONTEXT.NAME` alias.
- Verify `wt new` has OpenSSH's exit status, demonstrating process replacement
  rather than a successful return after launching a child.
- Verify the managed SSH inventory is written before OpenSSH starts.

## Consequences

- Creating a world immediately enters its Byobu workspace.
- Exiting or disconnecting from SSH ends the original `wt new` invocation with
  OpenSSH's status.
- Users can reconnect through the same managed alias; Byobu continues to own
  the persistent terminal workspace.
- Callers cannot use successful `wt new` completion as a boundary before the
  interactive SSH session ends. This is consistent with `wt new` being an
  interactive-only command.

## Alternatives

### Print the SSH command and return

Rejected because it requires a second manual command before the user can enter
the world.

### Spawn SSH and wait

Rejected because `wt` would remain as an idle wrapper and would need to proxy
process lifecycle behavior already provided by `exec`.
