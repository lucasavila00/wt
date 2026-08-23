# ADR 0010: Limit Rust files to 700 lines

- Status: Accepted
- Date: 2026-08-14

## Context

Several Rust source files have grown large enough that they mix multiple
responsibilities and are difficult to navigate, review, and change safely.
Without an enforced boundary, new code can continue accumulating in those
files and make later separation more expensive.

The repository also contains large generated schemas and dependency metadata.
Their size is determined by external formats and tools, so the same boundary
would not improve their maintainability.

## Decision

Limit every Rust file in the repository to 700 lines. Files with exactly 700
lines are allowed; files with more than 700 lines are rejected.

Provide `make check-file-lines` as the repository check. It discovers Rust
files with `rg --files -g '*.rs'`, counts their lines, reports every violation,
and exits unsuccessfully when any violation exists. The check has no
grandfathered Rust files or directory-specific exceptions.

Run the check continuously through both of the repository's mandatory
verification paths:

- The repository's pre-commit Git hook runs it for every local commit.
- CI runs it for every proposed change and every change to the default branch.

These integrations invoke the same Make target so local and CI enforcement
cannot drift. Running the check manually is useful for feedback, but is not the
enforcement mechanism.

## Consequences

- Existing Rust files above the limit must be split before the check passes.
- New production and test code share the same limit.
- Commits and CI builds are blocked whenever any Rust file exceeds the limit.
- Large generated non-Rust files and dependency metadata remain outside this
  check.
- Splitting a file requires choosing cohesive module boundaries instead of
  moving lines solely to satisfy the count.
