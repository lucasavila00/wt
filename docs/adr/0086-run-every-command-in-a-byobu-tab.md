# ADR 0086: Run every command in a Byobu tab

- Status: Accepted
- Date: 2026-09-02

## Decision

Each WT command is one Byobu tab. WT creates one Byobu window with one pane and starts the command
in that pane. The public API calls this object a command. Byobu presents the same object to the user
as a tab. The command ID addresses both.

A command and its Byobu tab share one lifecycle. The command provides:

- lifecycle state and an exit status;
- ordered tab input and output for programmatic clients; and
- the current tab screen as text, with its observation time.

The stable `wt api` operations are:

```text
start_command(world_id, argv, cwd) -> command
get_command(command_id, after, limit) -> state, exit_status, output, next_after, screen
send_command_input(command_id, input)
stop_command(command_id)
delete_command(command_id)
```

`after` is an exclusive output cursor. Repeating a read is safe. The screen is a bounded plain-text
rendering of the Byobu tab. WT keeps the tab after the command exits so its exit status and final
screen remain available. `delete_command` closes the tab and deletes its retained state.

## Ownership

WT owns the Byobu tab, command launch, input and output, status, screen rendering, and cleanup. A
client chooses the executable, arguments, and working directory.

Provider integrations remain client-owned. For example, Apr starts Codex in a command tab,
exchanges the Codex App Server protocol through that command's input and output, and uses the tab
screen for inspection and failure reports. WT applies the same behavior to every executable.

## Execution model

`start_command` returns after WT creates the Byobu tab and starts its command. The command continues
across client disconnects. `get_command` supports polling through the initial one-request `wt api`
client. A later client daemon can stream or reuse connections while preserving these operations.

Stopping or deleting a world stops its command tabs. WT bounds retained output and reports the
oldest available cursor when a reader falls behind.
