# ADR 0086: Unify command and terminal lifecycle

- Status: Accepted
- Date: 2026-09-02

## Decision

WT exposes one command resource for execution inside a world. Every command runs as the foreground
process in a dedicated PTY-backed Byobu window. Execution and terminal interaction use the same
command ID and lifetime.

A command provides:

- lifecycle state and an exit status;
- ordered input and output for programmatic clients; and
- the current rendered terminal screen with its observation time.

The stable `wt api` operations are:

```text
start_command(world_id, argv, cwd) -> command
get_command(command_id, after, limit) -> state, exit_status, output, next_after, screen
send_command_input(command_id, input)
stop_command(command_id)
delete_command(command_id)
```

`after` is an exclusive output cursor. Repeating a read is safe. The screen is a bounded plain-text
rendering of the command's Byobu pane. A completed command retains its final state and screen until
the command or its world is deleted.

## Ownership

WT owns command launch, PTY and Byobu setup, input and output transport, status, screen rendering,
and cleanup. A client chooses the executable, arguments, and working directory.

Provider integrations remain client-owned. For example, Apr starts a Codex command, exchanges the
Codex App Server protocol through the command input and output, and uses the same command screen for
inspection and failure reports. WT applies the same behavior to every executable.

## Execution model

`start_command` returns after WT creates the command and its Byobu window. The command continues
across client disconnects. `get_command` supports polling through the initial one-request `wt api`
client. A later client daemon can stream or reuse connections while preserving these operations.

Stopping or deleting a world stops its commands. Deleting a command removes its retained output
and screen. WT bounds retained output and reports the oldest available cursor when a reader falls
behind.
