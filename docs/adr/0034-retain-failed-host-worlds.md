# ADR 0034: Retain failed host worlds

- Status: Accepted
- Date: 2026-08-15
- Amends: [ADR 0026](0026-make-world-kinds-first-class.md)

## Context

WT deleted a host world when cloud-init failed. The create command showed the
error once, but the world disappeared from `wt ls` and could not be inspected or
deleted explicitly.

The CLI also hid cloud-init output until creation finished. This made slow or
failed recipes hard to understand.

## Decision

`wt new host` streams cloud-init stdout and stderr while the recipe runs. The
CLI writes this progress to stderr. `wt-server api` keeps the protocol v1 JSON
response on stdout. The server also writes the progress to its journal. Closing
the CLI does not cancel provisioning.

A failed host remains in `error`. WT keeps its domain, disk, NoCloud files,
failure text, and capacity reservation. The create command says the host was
kept. `wt ls` shows the failure and the `wt rm` command.

`wt start` and another create with the same name remain conflicts. `wt rm` is
the recovery path and removes all retained state. `make nuke` remains the full
reset.

Devcontainer creation is unchanged. Cloud-init output and failed host user-data
may remain on disk until deletion. Recipes must not print or contain secrets.
