# ADR 0011: Build world images in KVM

- Status: Accepted
- Date: 2026-08-14

The installer builds the host golden image in a temporary KVM guest from a
pinned Ubuntu source image. Rust owns disk, domain, progress, timeout,
sanitization, provenance, publication, and cleanup. Reviewable shell assets own
the whole-machine installation procedure.

The recipe writes a strict root-owned result marker only after installation and
validation succeed. The installed manifest records the base image, install
configuration, every staged input, resolved package versions, and final image
checksum. Build-only packages and bootstrap state are removed before
publication.

One exclusive build lock and reserved domain prevent overlapping builds.
Failures retain the primary diagnostic, include the console tail, and clean up
only the exact temporary build state. World disks are qcow2 overlays backed by
a pinned published image generation and must be at least as large as the image.
