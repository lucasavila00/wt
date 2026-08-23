# ADR 0008: Make clear the fast runtime reset

- Status: Accepted
- Date: 2026-08-10

## Context

`make clear` is the normal reset before reinstalling WT. It removes worlds and
generated runtime configuration. Installed golden images are verified against
their provenance manifests and are expensive to rebuild, but are not runtime
state.

## Decision

`make clear` removes all generated runtime state: worlds, disks, grants, the
database, generated SSH inventory, and `/etc/wt`. It preserves installed golden
images and their manifests, installed services and credentials, source
credential files, downloaded image and package artifacts, and the registry
cache.

After `make clear`, reinstall WT from the chosen install input. The installer
verifies preserved images before using them, points configuration drift to this
path, and atomically replaces preserved service definitions when they changed.

`make nuke` remains the full teardown. Use it when installed services,
encrypted credential copies, golden images, downloaded artifacts, or cache
state must also be removed.
