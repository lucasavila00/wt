# ADR 0020: Conservatively refresh prewarm generations

- Status: Deferred
- Date: 2026-08-09

## Context

A new commit does not always require rebuilding a devcontainer image. Checking
only whether the Dockerfile changed is still wrong: it may copy any
non-ignored file in its build context, and Compose may use includes,
environment, and interpolation.

VS Code has no automatic invalidation oracle. WT must make this choice without
a user pressing **Rebuild Container**.

## Decision

Reuse prepared state only when WT can prove its inputs are unchanged. An
unknown input is a cache miss, never a speculative hit.

Record three versioned fingerprints for every ADR 0019 generation:

- **Image:** devcontainer image settings, Dockerfiles, build arguments and
  targets, whole effective build contexts after ignore rules, Compose build
  settings, locked Features, builder versions, and resolved image digests.
- **Runtime:** the resolved Compose model and its includes, interpolation,
  profiles, configs, secrets, mounts, and services, or the equivalent
  non-Compose configuration.
- **Content:** the exact Git commit and lifecycle commands.

Missing files, escaping paths, undeclared interpolation, dynamic configuration,
or unsupported frontends make a fingerprint unknown. Secret changes must
invalidate it without storing plaintext. Mutable image tags are resolved on
every check. Build-time secret and SSH mounts are unsupported initially.

| Change | Action |
|--------|--------|
| Nothing | Keep the generation |
| Content only, with `updateContentCommand` | Clean and update the checkout; run the command |
| Content or lifecycle without a safe update | Recreate runtime from existing images; run the create lifecycle |
| Runtime | Recreate runtime from existing images |
| Image or unknown input | Run BuildKit with cache, then recreate runtime |

Hashing the whole build context may invoke BuildKit for a file the Dockerfile
does not use. That is acceptable. BuildKit knows the real instruction graph and
will reuse unaffected layers. WT will not parse Dockerfiles or use `--no-cache`
for a normal refresh.

Normal BuildKit rules still apply. Cached remote work such as `apt-get update`
does not rerun merely because the remote data changed. Projects must pin that
input or provide an explicit cache-busting argument.

## Verification

- Identical fingerprints do not invoke BuildKit.
- Relevant image inputs invoke BuildKit; ignored files do not.
- Equivalent Compose input normalizes identically.
- Unknown inputs always take the conservative path.
- Diagnostics never contain plaintext secrets.

## Consequences

WT may invoke BuildKit more often than necessary, but BuildKit still reuses
valid layers. This is safer than maintaining a second Dockerfile parser.

## Alternatives

Watching only Dockerfile and Compose paths misses their inputs. Parsing
Dockerfiles duplicates BuildKit. Always using `--no-cache` throws away safe
reuse.

## References

- [Docker build contexts](https://docs.docker.com/build/concepts/context/)
- [Docker cache invalidation](https://docs.docker.com/build/cache/invalidation/)
- [Canonical Compose configuration](https://docs.docker.com/reference/cli/docker/compose/config/)
