# ADR 0012: Separate image packages from world configuration

- Status: Accepted
- Date: 2026-07-31
- Amended by: [ADR 0027](0027-build-images-in-kvm.md) and
  [ADR 0043](0043-own-retained-guest-foundation-in-shared-images.md)

## Context

WT creates every world from one retained golden image. Some packages need a
version Ubuntu 24.04 does not provide.

Downloading those packages again for every world would be slow and would make
world creation depend on the external artifact still being available.

## Decision

The golden-image recipe owns the machine, terminal, and WT integration
packages. Normal packages come from Ubuntu; exceptional artifacts are pinned
by version, URL, and SHA-256.

Installed contract packages stay in the image manifest so world provisioning
can require their exact versions.

The image owns the retained `wt` user and terminal files. Runtime provisioning
supplies only world-specific access, Git identity, and gateway credentials. It
does not download pinned image artifacts or install application packages.

The image compatibility field remains `1`. Staged-input hashes detect recipe
changes.

Installing the expected version is not enough to prove compatibility. Run the
real KVM E2E after rebuilding.

## Consequences

- Packages are downloaded once per golden-image generation, not once per world.
- A bad or missing pinned artifact fails the image build early.
- User configuration remains independent from package installation.
- Package upgrades require a real-system behavior check, not only an image
  build and version check.
- WT owns maintenance and security updates for externally pinned packages.
- Pinning a `.deb` does not pin dependencies supplied by Ubuntu.
