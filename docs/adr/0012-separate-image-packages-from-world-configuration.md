# ADR 0012: Separate image packages from world configuration

- Status: Accepted
- Date: 2026-07-31
- Amended by: [ADR 0027](0027-build-images-in-kvm.md) and
  [ADR 0043](0043-own-retained-guest-foundation-in-shared-images.md)

## Context

WT installs shared packages in kind-specific images, then creates each world
from its image. Some packages need a version Ubuntu 24.04 does not provide.

Downloading those packages again for every world would be slow and would make
world creation depend on the external artifact still being available.

## Decision

The shared image recipe owns common machine and terminal packages. Kind recipes
own their application packages. Normal packages come from Ubuntu; exceptional
artifacts are pinned by version, URL, and SHA-256.

Installed contract packages stay in the image manifest so world provisioning
can require their exact versions.

The shared image foundation owns the retained `wt` user and shared terminal
files. Kind provisioners own kind-specific per-world services, credentials, and
configuration. They do not download pinned image artifacts or create a second
retained guest user.

The image compatibility field remains `1`. Staged-input hashes detect recipe
changes.

Installing the expected version is not enough to prove compatibility. Run the
real KVM E2E after rebuilding.

## Consequences

- Shared packages are downloaded once per kind image, not once per world.
- A bad or missing pinned artifact fails the image build early.
- User configuration remains independent from package installation.
- Package upgrades require a real-system behavior check, not only an image
  build and version check.
- WT owns maintenance and security updates for externally pinned packages.
- Pinning a `.deb` does not pin dependencies supplied by Ubuntu.
