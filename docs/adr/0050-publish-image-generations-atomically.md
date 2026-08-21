# ADR 0050: Publish image generations atomically

- Status: Proposed
- Date: 2026-08-21

## Context

The retained image and its provenance manifest form one runtime input.
Publishing them with separate moves exposes an incomplete or mismatched pair.

## Decision

The configured image path remains the legacy image path.

Each new generation lives under `<image>.generations/<id>/` and contains the
image and its adjacent provenance manifest.

`<image>.current` is a relative symlink to one complete generation directory.
The installer stages the directory and symlink under temporary names, then
switches `.current` with one `mv -T`.

The server resolves `.current` once at startup. The resolved image path and its
adjacent manifest stay pinned for that process.

A verified legacy pair is hard-linked into the first generation. Legacy files
and old generations remain available to running older and pinned servers.

## Consequences

- Readers see either the old complete generation or the new complete generation.
- Image rebuilds do not invalidate a running server's pinned generation.
- Generation cleanup requires coordination with server process lifetimes.
