# ADR 0034: Retain failed host worlds

- Status: Accepted
- Date: 2026-08-15
- Amends: [ADR 0026](0026-make-world-kinds-first-class.md)

## Context

WT deleted a host world when cloud-init failed. The create command showed the
error once, but the world disappeared from `wt ls` and could not be inspected or
deleted explicitly.

## Decision

A host provisioning failure leaves the world in `error`. WT keeps its domain,
disk, NoCloud files, failure text, and capacity reservation. The create command
says the world was retained. `wt ls` shows the failure and its `wt rm` command.

`wt start` and another create with the same name remain conflicts. `wt rm` is
the recovery path and removes all retained state. `make nuke` remains the full
reset.

Devcontainer create cleanup is unchanged. Failed host user-data remains in
plaintext until deletion and must not contain secrets.
