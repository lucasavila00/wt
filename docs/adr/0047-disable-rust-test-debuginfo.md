# ADR 0047: Disable Rust test debuginfo

- Status: Accepted; Date: 2026-08-23

## Problem

The complete pre-merge check runs the Rust workspace tests after separate
`cargo check` and Clippy passes. On a fresh four-job development guest, building
the unoptimized test binaries with full debuginfo took over thirteen minutes,
while executing the tests took roughly one minute. Compilation and linking,
not test execution, dominate the feedback time.

Running tests with Cargo's release profile would optimize the test binaries,
but that makes a fresh build more expensive and changes debug-mode behavior.
The test suite does not need embedded debugger information in its binaries;
test failures still report names, assertions, and Rust backtraces without it.

## Decision

Keep the standard unoptimized test profile and set `profile.test.debug` to
zero at the workspace root. Continue using `cargo test --workspace --locked`
for the regular CI suite.

Do not use the release profile for the regular test suite. Release-mode tests
remain available as a targeted diagnostic when behavior depends on
optimization.

## Consequences

- Fresh test builds spend less time generating and linking debug information.
- Test artifacts consume less disk space.
- Tests retain debug assertions and unoptimized behavior.
- Developers who need source-level debugging must temporarily override the
  test profile locally, for example with `CARGO_PROFILE_TEST_DEBUG=2`.
- Performance-sensitive code still needs explicit release-mode benchmarks or
  targeted release-mode tests; the regular suite does not validate optimized
  runtime behavior.
