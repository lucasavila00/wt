# ADR 0069: Use one development-tools golden image

- Status: Accepted
- Date: 2026-08-23

## Context

The host-local cache makes repeated development-tools image builds practical.
E2E should exercise the same golden image that servers publish.

## Decision

- Every golden image includes the development tools and uses the shared cache.
- Remove `image.development_tools`; there is no slim image profile.
- KVM E2E validates the development tools in a real world.

## Consequences

- Cold builds download and install the tools; warm builds reuse the cache.
- Every world includes Docker and the development toolchains, with the larger
  image, disk use, services, and attack surface that entails.
