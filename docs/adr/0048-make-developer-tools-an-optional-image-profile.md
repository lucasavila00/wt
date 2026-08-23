# ADR 0048: Make developer tools an optional image profile

- Status: Accepted
- Date: 2026-08-23

## Context

WT's retained golden image is shared by every new world. Current Rust/Cargo,
Go, Python/uv, Node.js/npm through NVM, build tools, CLI utilities, and Docker/Compose are
useful for development, but they add substantial download time and image size.
The KVM E2E path creates images from scratch and must not pay that cost for
tests that do not need those tools.

"Latest" is a build-time requirement: upstream releases change independently
of WT. Pinning each release in source would require frequent maintenance and
would make a newly built developer image stale immediately.

## Decision

Add `image.development_tools`, defaulting to `false`, to the install and
runtime image configuration. When enabled, the golden-image recipe installs:

- current stable Rust/Cargo (including rustfmt and Clippy), Go, Python through
  uv, and Node.js/npm through NVM, with Node.js, npm, npx, and Corepack on the
  default command path;
- `make`, CMake, GCC, Clang, and pkg-config;
- curl, wget, jq, yq, ShellCheck, Docker, and Docker Compose.

The recipe obtains the current upstream releases while building, verifies the
Go archive checksum published by go.dev, and records every resolved tool
version in image provenance. The package manifest and provenance option are
validated on reuse, so changing the option rebuilds the image rather than
silently reusing a different profile.

The enabled world shell announces the high-level tool inventory. The default
and KVM E2E install inputs leave the option disabled.

## Consequences

- Operators can choose a ready-to-use developer environment without changing
  world provisioning or downloading tools per world.
- Default image creation and KVM E2E remain narrow and avoid third-party
  language-runtime downloads.
- Developer-image rebuilds depend on upstream availability and can produce
  newer tool versions for the same WT commit; their manifest makes the exact
  result auditable.
- Enabling or disabling the option creates a distinct retained image
  generation; existing worlds continue to use their current generation.
