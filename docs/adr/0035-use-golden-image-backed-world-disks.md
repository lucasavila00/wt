# ADR 0035: Use golden-image-backed world disks

- Status: Accepted; Date: 2026-08-22

## Context

Copying the complete golden image for every world would move gigabytes of data
onto the `wt new` critical path. Published image generations provide stable
paths that world disks can safely reference, provided referenced generations
remain installed.

## Decision

Create each world disk as a qcow2 overlay whose backing file is the absolute
path of the image generation pinned by the running server. Do not copy the
golden image during world creation.

Keep published image generations. An image generation must not be removed
while any world overlay or running server may reference it. Image publication
changes the current-generation pointer for a subsequently restarted server;
it does not rewrite existing overlays.

## Consequences

- World disk creation is an effectively constant-time metadata operation.
- Existing worlds keep their contents across image publication and server
  restart because their overlays name a specific retained generation.
- A world disk is not self-contained. Moving it requires its backing image or
  an explicit flattening operation.
- Image-generation cleanup requires reference discovery or an explicit
  migration design; deleting generations blindly can break worlds.
