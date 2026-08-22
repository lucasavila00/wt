# ADR 0058: Restore golden-image-backed world disks

- Status: Accepted; Date: 2026-08-22
- Supersedes: [ADR 0039](0039-make-world-disks-independent-of-golden-images.md)
- Amends: [ADR 0050](0050-publish-image-generations-atomically.md)

## Problem

ADR 0039 made each new world disk independent by converting the complete
golden image into a new qcow2 file. That synchronous copy moved roughly 3 GiB
of allocated image data onto the `wt new` critical path. World creation
regressed from approximately 10 seconds to as much as 60 seconds, with
`qemu-img convert` dominating the delay before the guest could boot.

Atomic image generations now provide a stable path for every published golden
image. The independent copy is therefore not required to protect a world from
replacement of the current-generation pointer, provided that referenced image
generations remain installed.

## Decision

Create each world disk as a qcow2 overlay whose backing file is the absolute
path of the image generation pinned by the running server. Do not copy the
golden image during world creation.

Keep published image generations. An image generation must not be removed
while any world overlay or running server may reference it. Image publication
changes the current-generation pointer for a subsequently restarted server;
it does not rewrite existing overlays.

## Consequences

- World disk creation no longer copies gigabytes before boot and returns to an
  effectively constant-time metadata operation.
- Existing worlds keep their contents across image publication and server
  restart because their overlays name a specific retained generation.
- A world disk is not self-contained. Moving it requires its backing image or
  an explicit flattening operation.
- Image-generation cleanup requires reference discovery or an explicit
  migration design; deleting generations blindly can break worlds.
