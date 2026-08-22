# ADR 0066: Use the commit for golden-image reuse

- Status: Accepted; Date: 2026-08-22
- Amends: [ADR 0059](0059-bake-static-guest-binaries-into-golden-images.md)

## Problem

The retained golden-image manifest recorded hashes for the source image,
rendered server configuration, every staged build asset, every guest binary,
and the finalized tmux configuration. Reuse required all of them to match.

This selective invalidation machinery duplicated knowledge of the image build
across staging and verification. Every new build input had to be added to the
fingerprint explicitly, so the apparent precision also created a maintenance
risk: an omitted input could incorrectly reuse an old image.

WT releases are built from a known Git commit, and rebuilding once per commit
is an acceptable tradeoff. Existing installations and worlds do not require a
compatibility migration for this manifest change; they may be removed with
`make nuke` and recreated.

## Decision

Use the WT Git commit recorded in the golden-image manifest as the sole cache
identity. Reuse an installed image only when that commit exactly matches the
running installer. A different commit always rebuilds the image, including
when the commit changes only documentation or unrelated code.

Remove the source-image, rendered-configuration, staged-input, guest-binary,
and tmux hashes from the reuse identity and manifest. Do not support the old
manifest shape.

Continue checking the published image's SHA-256 digest to detect corruption.
Continue validating package versions, the fixed guest identity, file metadata,
and the guest contract before publication or reuse. Download and build-time
checksums also remain local integrity checks; none of these checks decide
whether two source revisions share a cache identity.

## Consequences

- The reuse rule is one comparison that is easy to explain and audit.
- Image build inputs cannot change across commits without forcing a rebuild.
- A commit that does not affect the image still incurs one rebuild.
- Changing local installation configuration without changing the commit does
  not invalidate an existing image. Operators must explicitly rebuild when
  testing such uncommitted configuration changes.
- Old manifests fail parsing and cause automatic replacement when installing;
  operators may use `make nuke` when recreating an existing deployment.
