# ADR 0068: Reuse a host-local development-tools base image

- Status: Accepted
- Date: 2026-08-23

## Context

Do not redownload and reinstall development tools when only WT-shipped code
changes.

## Decision

- `wt-server-installer` owns one disposable host-local base image and manifest
  in its working directory's `imgs/` directory.
- Every golden-image build uses it. Worlds do not consume it directly.
- The cache is a sanitized image base, not a runtime or distributed cache. Each
  final golden image is a separate copy with the current WT guest layer and all
  normal validation reapplied.
- Reuse requires a complete image/manifest pair, its recorded checksum, and
  matching source image, build disk size, and development-tools recipe.
  Missing, corrupt, or incompatible entries are rebuilt.
- A cache generation fixes its selected tool versions. Normal WT changes reuse
  it; changing cache inputs or removing it refreshes tools from upstream.
  Final-image provenance records the resolved versions.

## Consequences

- Cold builds install the tool layer; warm rebuilds reuse it and still rebuild
  the terminal, Codex, and WT layers.
- Tool upgrades are deliberate. Cached tools can lag upstream until refresh.
- `make clear` preserves the cache; checkout `make nuke` removes its cache.
- The cache is integrity-checked, not a security boundary; it is trusted local
  state owned by the server user.
