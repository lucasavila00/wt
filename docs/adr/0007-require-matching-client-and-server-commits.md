# ADR 0007: Require matching client and server commits

- Status: Proposed
- Date: 2026-07-17

## Context

Different `wt` and `wt-server` commits may use incompatible protocols. WT does
not support mixed versions.

## Decision

At build time, embed the WT repository's full Git commit hash in both binaries.
Fail the build if the hash cannot be read.

The client sends its hash with every request. The server compares it with its
own hash before running the operation. Missing, malformed, or different hashes
are rejected without changing state.

The error shows both hashes and tells the user to install matching builds. The
check applies to every operation over local and OpenSSH transports.

## Consequences

- Client and server must be upgraded together.
- Every commit difference is incompatible, even when the protocol did not
  change.
- Equal hashes do not detect uncommitted changes or different build settings.
