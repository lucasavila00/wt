# ADR 0018: Make clear the fast runtime reset

- Status: Accepted
- Date: 2026-08-10

## Context

`make clear` is the normal reset before reinstalling WT. It removes worlds and
installed world images but keeps downloads and the registry cache.

Before this decision, `make clear` kept `/etc/wt/server.toml`. A changed runtime
schema therefore required `make nuke`, which also discarded the caches.

## Decision

`make clear` removes all generated runtime state: worlds, disks, grants, the
database, generated SSH inventory, installed world images, and `/etc/wt`. It
preserves installed services and credentials, source credential files,
downloaded image and package artifacts, and the registry cache.

After `make clear`, reinstall WT from the chosen install input. The installer
points configuration drift to this path and atomically replaces preserved
service definitions when they changed.

`make nuke` remains the full teardown. Use it when installed services,
encrypted credential copies, downloaded artifacts, or cache state must also be
removed.
