# ADR 0048: Limit snapshots and cover every wt-tools command

- Status: Accepted
- Date: 2026-08-21
- Amends: [ADR 0047](0047-make-agent-git-output-json-only.md)

## Context

Snapshots throughout the repository review stable generated output, diagnostics,
configuration, scripts, and service units. Very large snapshots are difficult to
review and can hide accidental output growth.

`wt-tools` returns typed provider results as JSON. Removing hand-written text
formatters also removes a natural point where fields are selected explicitly.
An implementation could accidentally serialize a large upstream response or an
unbounded collection, making agent transcripts expensive and difficult to use.

Tests cover individual provider behaviors, but they do not establish that every
public command has a reviewed JSON output contract. Separate handwritten command
and output test lists can drift when commands are added.

Production JSON is compact, so its physical line count is always close to one.
A useful structural size check must parse and pretty-print the response before
counting lines.

## Decision

Limit every committed `.snap` file in the repository to 1,000 lines. Enforce the
limit in the static checks with a small Rust command that reads each complete
snapshot file. Allow a small number of exceptions only as exact
repository-relative paths in a reviewed allowlist. Use external snapshots for
large values so the limit applies to a distinct, reviewable file.

Maintain one data-driven test table containing every `wt-tools` JSON command.
Use it both for command parsing tests and for one named snapshot of each
command's complete, versioned JSON response. Use an ordinary exhaustive match on
`CliCommand` to select representative output, so adding a variant without an
output mapping fails to compile.

Parse each rendered response back into JSON and snapshot its complete
pretty-normalized value. Reject any command fixture whose normalized response is
more than 1,000 lines. Keep direct byte limits for opaque multiline values such
as CI logs, because JSON escaping makes their source line count invisible to the
structural limit.

Use representative nested data rather than empty collections so snapshots expose
the serialized fields of merge requests, reviews, CI runs, and CI jobs.

## Consequences

- Every snapshot remains bounded by default, regardless of its crate or format.
- Exceptional large snapshots are visible in one explicit allowlist.
- Every command addition requires an explicit, reviewed output snapshot.
- Schema changes are visible as complete snapshot diffs.
- Large serialized collections fail the structural line guard.
- Snapshot fixtures do not replace provider behavior tests or runtime limits on
  opaque strings.
- The test table duplicates the public command spellings intentionally while
  deriving parsing and output coverage from one source.

## Alternatives

Applying the limit only to `wt-tools` is rejected because accidental output growth
is a repository-wide review problem. Checking compact output line counts is
rejected because compact JSON is normally one line regardless of structural size.
A byte-only limit is rejected because it is less readable in review and does not
provide a schema contract. Maintaining independent parser and output case lists
is rejected because they can drift.
