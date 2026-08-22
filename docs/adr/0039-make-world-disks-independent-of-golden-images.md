# ADR 0039: Make world disks independent of golden images

- Status: Superseded by [ADR 0058](0058-restore-golden-image-backed-world-disks.md)
- Date: 2026-08-16
- Amended by: [ADR 0043](0043-own-retained-guest-foundation-in-shared-images.md)

## Context

Golden images must remain replaceable build artifacts. A world disk must be
self-contained so its lifetime and availability do not depend on the installed
template.

## Decision

Treat golden images as templates, not runtime dependencies. Creating a world
copies and resizes the golden image into an independent qcow2 disk.

When an installed golden image is missing, invalid, or stale, server setup
builds and publishes its replacement automatically. Publication remains staged:
a failed build leaves the installed image unchanged. Existing worlds do not use
the replaced file and keep running.

The same rule applies when the shared retained-image foundation changes:
golden-image replacement does not rewrite existing world disks. Recreate
affected worlds to receive the new image-owned guest user and terminal
contract.

## Consequences

- Routine upgrades need one install command and do not stop existing worlds.
- Initial world creation copies the allocated image data and can take longer.
- Golden-image replacement no longer needs an active-domain check.
