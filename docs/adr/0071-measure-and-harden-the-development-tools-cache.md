# ADR 0071: Measure and harden the development-tools image cache

- Status: Accepted
- Date: 2026-08-23
- Amends: [ADR 0068](0068-cache-development-tools-image-layer.md)

## Context

The first real KVM runs exposed two problems.

The warm-build log said it was copying the cache for more than two minutes. That
message covered several operations, so it did not show where the time went. An
instrumented warm KVM build showed that cloning the 5.2 GiB allocated, 32 GiB
virtual cache with `qemu-img convert` took 126.2 seconds. Final-image compaction
took another 173.7 seconds.

The cache also had correctness gaps:

- tool discovery output was written before the version manifest;
- changing `.nvmrc` did not invalidate the cache;
- a malformed manifest stopped the build instead of rebuilding the cache;
- an abandoned `.qcow2.new` could block later builds.

## Decision

Keep one sanitized, host-local development-tools image. It remains a standalone
qcow2 file. Worlds never use it directly, and published golden images must not
depend on it as a backing file.

Report elapsed time for the expensive image phases: cache verification, disk
creation or clone, libguestfs staging, guest execution, sanitization, compaction,
hashing and publication, and the final boot probe. A message covers exactly one
phase.

The cache identity includes:

- a cache schema version;
- source-image checksum and build-disk size;
- guest identity;
- the pinned Node version from `.nvmrc`;
- the scripts that install and sanitize the cached layer.

The tool manifest contains only `name<TAB>version` lines. Validation commands
must not write to it.

Missing, incomplete, malformed, stale, or checksum-mismatched cache state is
discarded and rebuilt. An abandoned temporary publication is removed while the
exclusive image-build lock is held. Failure cleanup removes only that temporary
file and the failed build state.

Do not add a qcow backing chain. Clone the cache with a filesystem reflink when
the host supports it and fall back to an ordinary sparse copy. Both paths create
an independent file. The measured warm build spent 126 seconds in the previous
eager `qemu-img convert`, so this optimization is material on the KVM test host.

## Validation

Run one cold and one warm real KVM build. The cold build must publish a valid
cache. The warm build must reuse it without changing its checksum or manifest.
Both builds must publish a standalone final image and pass the real-world tool,
sanitization, provenance, and boot-probe checks.

On the KVM test host, the reflink change reduced the cache clone from 126.2
seconds to less than 0.1 seconds. The complete warm image build fell from 487.3
seconds to 339.7 seconds. The cache and manifest checksums were unchanged, and
both published images had no backing file.

## Consequences

Image builds explain their own latency. Cache invalidation follows every input
that changes installed tools, and interrupted cache publication repairs itself
on the next build.

The final image is still copied and sanitized independently. This costs more than
a permanent backing chain but keeps checkout cleanup and world lifetime separate.
