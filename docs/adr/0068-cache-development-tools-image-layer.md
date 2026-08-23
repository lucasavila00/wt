# ADR 0068: Reuse a host-local development-tools base image

- Status: Accepted
- Date: 2026-08-23
- Amends: [ADR 0067](0067-make-developer-tools-an-optional-image-profile.md)

## Context

Do not redownload and reinstall development tools when only WT-shipped code
changes.

## Decision

- `wt-server-installer` owns one disposable host-local base image and manifest
  in the installer checkout's `imgs/` directory.
- Only enabled `image.development_tools` builds use it. Worlds, default images,
  and KVM E2E do not.
- The cache is a sanitized image base, not a runtime or distributed cache. Each
  final golden image is a separate copy with the current WT guest layer and all
  normal validation reapplied.
- Reuse requires a complete image/manifest pair, its recorded checksum, and
  matching source image, build disk size, terminal pins, and cached recipe.
  Missing, corrupt, or incompatible entries are rebuilt.
- A cache generation fixes its selected tool versions. Normal WT-only changes
  reuse them; changing compatibility inputs or removing the cache refreshes
  tools from upstream. Final-image provenance records the resolved versions.

## Consequences

- The disabled profile has no cache cost. First enabled builds pay the full
  install cost plus one local image; warm rebuilds pay copy and validation only.
- Tool upgrades are deliberate. Cached tools can lag upstream until refresh.
- `make clear` preserves the cache; `make nuke` removes it.
- The cache is integrity-checked, not a security boundary; it is trusted local
  state owned by the server user.
