# ADR 0046: Use the commit for golden-image reuse

- Status: Accepted; Date: 2026-08-22

## Context

Image reuse needs a source identity that cannot silently omit a new build input.
WT releases are built from a known Git commit, and rebuilding once per commit
is an acceptable tradeoff. The optional image profile remains an explicit
variant of that source revision.

## Decision

Use the WT Git commit recorded in the golden-image manifest as the source cache
identity. Reuse an installed image only when that commit exactly matches the
running installer and its recorded image profile matches the requested profile.
A different commit always rebuilds the image, including when the commit changes
only documentation or unrelated code.

Do not maintain a selective source fingerprint alongside the commit.

Continue checking the published image's SHA-256 digest to detect corruption.
Continue validating package and development-tool versions, the selected image
profile, the fixed guest identity, file metadata, and the guest contract before
publication or reuse. Download and build-time checksums also remain local
integrity checks; none of these checks decide whether two source revisions
share a source cache identity.

## Consequences

- The reuse rule is one comparison that is easy to explain and audit.
- Image build inputs cannot change across commits without forcing a rebuild.
- A commit that does not affect the image still incurs one rebuild.
- Changing the selected image profile invalidates an existing image even when
  the commit is unchanged.
