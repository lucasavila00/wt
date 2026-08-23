# ADR 0026: Publish image generations atomically

- Status: Accepted; Date: 2026-08-21

## Problem

An image and its manifest form one publication unit. Readers must never observe
an image from one build with a manifest from another.

## Decision

Put every image and its manifest in `<image>.generations/<id>/`.

Finish writing both files before making that directory current. Point
`<image>.current` at the finished directory. Replace that symlink with one
`mv -T`, so there is only one operation that changes what readers see.

The server resolves `.current` once when it starts. It keeps using that image
and manifest until the process restarts, even if the installer publishes a new
generation.

Do not migrate the old two-file layout. Run `make nuke` before upgrading an
existing installation. Keep old generations because a running server may still
use one.

## Consequences

- A failed publish leaves readers on the previous complete generation.
- A rebuild does not change the image used by an already-running server.
- Removing old generations needs a separate cleanup design.
