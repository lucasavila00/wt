# ADR 0018: Make clear the fast runtime reset

- Status: Accepted
- Date: 2026-08-10

## Context

`make clear` is the normal reset before reinstalling WT. It removes worlds and
the golden image but keeps the downloads and registry cache that are slow to
recreate.

It currently keeps `/etc/wt/server.toml`. A changed runtime schema therefore
requires `make nuke`, which also throws away the expensive caches.

## Decision

`make clear` removes all generated runtime state: worlds, disks, grants, the
database, generated SSH inventory, the golden image, and `/etc/wt`. It preserves
installed services and credentials, source credential files, downloaded image
and package artifacts, and the registry cache.

After `make clear`, reinstall WT from the chosen install input. The installer
points configuration drift to this path and atomically replaces preserved
service definitions when they changed.

`make nuke` remains the full teardown. Use it when installed services,
encrypted credential copies, downloaded artifacts, or cache state must also be
removed.
