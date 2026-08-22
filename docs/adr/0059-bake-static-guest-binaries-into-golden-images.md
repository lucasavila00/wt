# ADR 0059: Bake static guest binaries into golden images

- Status: Accepted; Date: 2026-08-22
- Amends: [ADR 0043](0043-own-retained-guest-foundation-in-shared-images.md)

## Problem

World provisioning copied four immutable musl executables into every new guest:
the agent-tool relay, Git remote helper, `wt-tools`, and Codex integration.
Those files were sent as base64 data through synchronous QEMU guest-agent RPCs
in 48 KiB chunks. The transfer and installation took approximately 18.6 seconds
even though the files were already built and every world started from a golden
image.

Larger RPC chunks reduce the call count, but retain avoidable work in the
creation path and proved unreliable on the supported host stack. The
executables change with a WT release, not with an individual world.

## Decision

Build and validate the four static executables before preparing the retained
image. Include their SHA-256 digests in the image provenance manifest, install
them root-owned with mode 0755, and verify their metadata and contents before
publishing the image. Run the Codex integration installer while building the
image so its user-owned trampoline is part of the same contract.

World provisioning transfers only mutable, world-specific data: SSH access,
Git identity, gateway grant and provider configuration, the vsock port, and
Codex mount state. It invokes image-owned helpers and does not replace
executables.

A change to any static guest executable produces a new image generation.
Existing world overlays retain their original generation. Image-only build and
publication workflows must be paired with a compatible host gateway release;
an incompatible protocol change requires compatibility support or recreation
of worlds on a coherent generation.

## Consequences

- New-world creation avoids more than one hundred serialized guest-agent write
  calls and the corresponding ownership and installation commands.
- Release installation does more work before image publication, where it can be
  validated once and reused by every new world.
- Golden images are coupled to a WT release through explicit binary hashes.
- Old image generations must remain available while world overlays reference
  them, including when their guest binaries differ from the current release.
