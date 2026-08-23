# ADR 0068: Reuse a host-local development-tools base image

- Status: Accepted
- Date: 2026-08-23
- Amends: [ADR 0067](0067-make-developer-tools-an-optional-image-profile.md)

## Context

The optional development-tools profile intentionally resolves current upstream
toolchains when it is built. WT changes more often than that tool layer needs
to change, so rebuilding a golden image for a new WT commit needlessly repeats
large downloads and installations.

The goal is to refresh WT in every golden image while retaining a deliberate,
auditable upgrade boundary for the development environment.

## Decision

`wt-server-installer` owns one disposable, host-local development-tools base
image and manifest in the installer checkout's `imgs/` directory. Only an
enabled `image.development_tools` golden-image build may create or use it.
Worlds, default images, and KVM E2E do not consume it.

The cache is a sanitized image base, not a runtime, package-manager, or
distributed cache. It contains no build instance's machine identity, SSH host
keys, or cloud bootstrap state. Every final golden image is a separate copy:
WT's current guest layer is applied again and the normal guest and
development-tools contracts are validated before that image is published.

A cache entry is reusable only when both its image and manifest exist, its
checksum matches the value recorded in that manifest, and its identity matches
the source image, build disk size, pinned terminal inputs, and cached image
recipe. Missing, partial, corrupt, or stale entries are replaced automatically.

A cache generation fixes the upstream development-tool versions selected when
it was created. Normal WT rebuilds reuse those versions. A change to its
compatibility inputs, or deliberate cache removal, refreshes the tools from
upstream; the resulting versions remain recorded in final-image provenance.

## Consequences

- The disabled profile has no cache cost. Its first enabled build still pays
  the full installation cost and retains an additional local image; later
  enabled rebuilds pay copy and validation cost, not tool installation cost.
- Development-tool upgrades are intentional rather than an incidental effect
  of a WT-only change. The tradeoff is that cached tools can lag upstream until
  the cache is refreshed.
- `make clear` preserves the cache; `make nuke` removes it with the rest of
  the installer checkout's image state. A fresh host therefore still performs
  a full enabled build.
- The cache is integrity-checked but is not a security boundary: its image and
  manifest are mutable host-local files trusted to the server user. Sanitizing
  it before publication prevents build-instance state from becoming its reuse
  contract.
