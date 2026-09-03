# ADR 0086: Keep Byobu windows as ephemeral guest runtime state

- Status: Accepted
- Date: 2026-09-02

## Decision

Byobu/tmux windows are guest runtime state, not WT resources. WT does not assign them IDs, store
them in SQLite, expose them through `wt api`, capture their input or output, or recover them after
a failure. The guest's tmux session is the source of truth.

A window exists only while its guest process and tmux session exist. Stopping or deleting a world,
or restarting its VM, destroys its windows. `wt shell` and SSH provide normal interactive access.

Programmatic process lifecycle, stdin/stdout delivery, output cursors, control tokens, and recovery
remain deliberately undecided. A later API requires a separate decision with concrete lifetime and
delivery guarantees.
