# ADR 0039: Make world disks independent of golden images

- Status: Accepted
- Date: 2026-08-16

## Context

World disks use the installed golden image as a live qcow2 backing file. Setup
therefore cannot replace a stale image while worlds exist, so an ordinary WT
upgrade stops and asks the operator to run a second command that may also be
blocked by those worlds.

## Decision

Treat golden images as templates, not runtime dependencies. Creating a world
copies and resizes the golden image into an independent qcow2 disk. Forks may
still use copy-on-write heads within that world's disk graph.

When an installed golden image is missing, invalid, or stale, server setup
builds and publishes its replacement automatically. Publication remains staged:
a failed build leaves the installed image unchanged. Existing worlds do not use
the replaced file and keep running.

There is no migration for old overlay-backed worlds. Clear them once before
installing this change.

## Consequences

- Routine upgrades need one install command and do not stop existing worlds.
- Initial world creation copies the allocated image data and can take longer.
- Golden-image replacement no longer needs an active-domain check.
