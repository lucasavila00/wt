# ADR 0034: Use JSON for agent Git commands

- Status: Accepted
- Date: 2026-08-16
- Amends: [ADR 0032](0032-make-agent-git-commands-explicit.md)'s positional command grammar

## Context

`ag-git` exposes a growing set of provider operations with positional arguments,
resource words, flags, and action-specific ordering rules. Its help text has
become a command matrix that agents must interpret rather than a precise input
contract. Adding alternate selectors, such as finding an open merge request by
branch, makes the positional grammar more ambiguous.

The guest binary already forwards its argument vector opaquely through the
relay. The gateway owns parsing and can therefore change the public command
encoding without changing the transport protocol or reinstalling existing guest
binaries.

## Decision

Require exactly one JSON object for every provider operation. Use an `action`
field as a discriminant, snake-case action names, typed action-specific fields,
and strict decoding that rejects unknown fields. Keep `ag-git help`, `--help`,
and `-h` as the only non-JSON invocations.

Publish the complete command contract in `ag-git help` as a TypeScript
discriminated union, including the `show_mr_for_branch` action. Continue to
render successful results and diagnostics as bounded human-readable text.

Do not retain the positional grammar as a second interface. Existing `ag-git`
binaries remain compatible because they can forward the single JSON argument;
only the gateway parser and help contract change.

## Consequences

- Every invocation is self-describing and can be validated without positional
  interpretation.
- New selectors and optional fields extend the union without creating command
  ordering rules.
- Shell callers must quote one JSON object.
- Old positional invocations fail with a pointer to the TypeScript contract.
- Output remains readable in agent transcripts and keeps existing size limits.
