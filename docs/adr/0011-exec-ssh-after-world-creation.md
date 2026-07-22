# ADR 0011: Exec SSH after world creation

- Status: Accepted
- Date: 2026-07-22

## Context

World creation finishes when the guest SSH endpoint is ready, but installation
starts only when a user connects to the managed `NAME` alias. `wt new`
currently prints a suggested SSH command and exits. This leaves an unnecessary
manual step between creating a world and observing its installation.

The SSH connection needs the workstation's terminal and SSH agent. It also
owns the interactive Byobu session in which installation runs and survives
later disconnects.

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

- Creating a world immediately enters its setup session and displays installer
  progress.
- Exiting or disconnecting from SSH ends the original `wt new` invocation with
  OpenSSH's status.
- Users can reconnect through the same managed alias; Byobu continues to own
  persistent installation and application sessions.
- Callers cannot use successful `wt new` completion as a boundary before the
  interactive SSH session ends. This is consistent with `wt new` being an
  interactive-only command.

## Alternatives

### Print the SSH command and return

Rejected because it requires a second manual command before installation can
start.

### Spawn SSH and wait

Rejected because `wt` would remain as an idle wrapper and would need to proxy
process lifecycle behavior already provided by `exec`.
