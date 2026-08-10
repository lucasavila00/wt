# ADR 0012: Separate image packages from world configuration

- Status: Accepted
- Date: 2026-07-31

## Context

WT installs shared packages in a golden image, then clones that image for each
world. Some packages may need a version Ubuntu 24.04 does not provide.

Downloading those packages again for every world would be slow and would make
world creation depend on the external artifact still being available.

## Decision

The golden-image recipe owns package installation. Normal packages come from
Ubuntu's repositories. Exceptional packages are pinned by version, URL, and
SHA-256, installed with `apt-get`, and verified before the image is published.

Installed contract packages stay in the image manifest so world provisioning
can require their exact versions.

`install-guest.sh` does not download exceptional packages. It owns per-world
state instead: the `wt` user, credentials, services, and user configuration.

When a pinned package changes, bump the image recipe version. Rebuild with
`make clear`, `make prepare-image`, and `make install-server`; `clear` keeps the
Ubuntu source image and registry cache. ADR 0018 makes `clear` the normal runtime
reset. Use `make nuke` only when installed service, credential, download, or
cache state must also be removed.

Installing the expected version is not enough to prove compatibility. Run the
real KVM end-to-end test after the rebuild. Put runtime overrides in the final
configuration layer owned by the package. For Byobu, that is
`~/.byobu/.tmux.conf`, which its profile sources last.

## Consequences

- Shared packages are downloaded once per image build, not once per world.
- A bad or missing pinned artifact fails the image build early.
- User configuration remains independent from package installation.
- Package upgrades require a real-system behavior check, not only an image
  build and version check.
- WT owns maintenance and security updates for externally pinned packages.
- Pinning a `.deb` does not pin dependencies supplied by Ubuntu.
