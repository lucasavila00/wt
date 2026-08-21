# ADR 0049: Use one image for retained worlds

- Status: Proposed; Date: 2026-08-21
- Amends: [ADR 0027](0027-build-images-in-kvm.md) and
  [ADR 0043](0043-own-retained-guest-foundation-in-shared-images.md)

## Context

The devcontainer image adds Docker Engine, Buildx, Compose, Node.js, npm, Git,
and the Dev Container CLI to the retained guest foundation. The host image
instead adds the host shell, cloud-init preparation and inspection scripts,
and the `wt-host-setup` systemd service.

## Decision

Build one retained-world image containing the devcontainer tools and host setup
assets. Devcontainer and host worlds will copy that image.

Replace the devcontainer and host image paths, build domains, manifests,
provenance, and result kinds with one retained-image contract.

World provisioning remains kind-specific. Host worlds will not create a
checkout or devcontainer; devcontainer worlds will not run host cloud-init
setup.

## Consequences

- Server installation builds and verifies one retained image.
- Host worlds include an unused Docker and Dev Container installation.
- Devcontainer and host image contents cannot drift.
