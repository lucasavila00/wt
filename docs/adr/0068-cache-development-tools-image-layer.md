# ADR 0068: Cache the development-tools image layer

- Status: Accepted
- Date: 2026-08-23
- Amends: [ADR 0067](0067-make-developer-tools-an-optional-image-profile.md)

## Problem

A new WT commit rebuilds the golden image. For the opted-in development-tools
profile, that repeats toolchain downloads and installation when the cached
layer has not changed.

## Decision

Build a local, sanitized development-tools cache image before the final enabled
golden image. Rebuilds copy the verified cache, refresh the WT-specific guest
layer, validate the tools, and publish a standalone image.

Reuse the cache only when its manifest identity and image checksum match. Its
identity includes the pinned Ubuntu source, build disk size, terminal pins, and
the recipe assets that install the cached layer. A changed identity, missing or
incomplete manifest, or checksum failure rebuilds the cache and obtains current
upstream tool releases.

The cache is used only by `image.development_tools = true`. It is not a world
runtime cache; default images and KVM E2E neither build nor copy it.

## Consequences

- The first enabled image build also publishes the cache, adding a small copy
  and verification cost.
- Later enabled rebuilds avoid reinstalling development toolchains; they still
  build and validate the WT-specific image layer.
- The final image provenance continues to record resolved tool versions.
