# ADR 0078: Consolidate WT executables by runtime

- Status: Accepted
- Date: 2026-08-23

## Context

The WT installer published seven executable files:
`wt-agent-tool-gateway`, `wt-agent-tool-gateway-relay`,
`git-remote-wt-agent`, `wt-tools`, `wt`, `wt-codex-integration`, and
`wt-server`. Most selected a mode of the same installed WT release. Separate
files multiplied build, validation, image-contract, installation, and upgrade
bookkeeping without isolating those modes at runtime.

Three compatibility and audience boundaries are real. The user-facing client
must run without server or guest dependencies. The server links the supported
libvirt ABI. Guest tools must be static executables that work in golden images.
External programs also require particular command names: Git discovers a
remote helper by name.

## Decision

Install three WT executables with distinct names:

| Executable | Runtime | Commands |
|------------|---------|----------|
| `wt` | user workstation | client and terminal workspace |
| `wts` | WT server | control daemon, API bridge, setup, and image management |
| `wtg` | WT guest | relay and agent tools |

`wts` is a GNU executable linked to the server's libvirt ABI. `wtg` is a
static musl executable baked into each guest image. `wt` remains the
user-facing client and does not acquire server or guest commands.

Long-running and internal entrypoints are explicit subcommands: `wts serve`,
`wtg relay` and `wtg tools`. Command parsing dispatches
into typed Rust library code instead of spawning another WT executable.

Install extra guest names only where an external invocation contract requires
one. They are symlinks to `wtg`, which dispatches by its invoked name:

- `git-remote-wt-agent` for Git's remote-helper discovery; and

Do not retain the former executable names as general compatibility shims. This
is an installation contract change deployed as one coherent server and guest
image generation.

The standalone Git proxy and its installer remain separate release
executables. They form an independently installable product with different
configuration and credentials, as established by ADR 0016. Development-only
test executables are outside this decision.

## Consequences

- The executable name states where it runs: client `wt`, server `wts`, or
  guest `wtg`.
- A normal server installation publishes `wts`; a guest image contains one
  `wtg` file plus the two protocol-required symlinks.
- Updating `wts` or `wtg` requires validating all of its command modes.
  Internal module boundaries and targeted tests remain.
- Scripts, service units, image contracts, and documentation migrate
  atomically; old guests continue to use the executable embedded in their
  original image generation.
