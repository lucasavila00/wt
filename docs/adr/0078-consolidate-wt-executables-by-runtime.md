# ADR 0078: Consolidate WT executables by runtime

- Status: Proposed
- Date: 2026-08-23

## Context

The WT installer currently publishes seven executable files:
`wt-agent-tool-gateway`, `wt-agent-tool-gateway-relay`,
`git-remote-wt-agent`, `wt-tools`, `wt`, `wt-codex-integration`, and
`wt-server`. Most select a mode of the same installed WT release. Separate
files multiply build, validation, image-contract, installation, and upgrade
bookkeeping without making those modes more isolated at runtime.

Some separation is real. The host server links the supported host libvirt ABI,
while guest tools must be static executables that work in golden images.
External programs also require particular command names: Git discovers a
remote helper by name, and the Codex wrapper must occupy the `codex` command.

## Decision

Build and install one WT executable per runtime compatibility boundary:

- a host GNU executable containing the CLI, server, and host installation
  commands; and
- a static musl guest executable containing the relay, Git remote-helper,
  agent tools, and Codex integration commands.

Both executables use the `wt` command tree. Long-running or internal entrypoints
are explicit subcommands, such as `wt server`, `wt guest relay`, and
`wt codex ...`; user-facing agent-tool operations remain under a single
`wt tools` command family. Command parsing dispatches into typed Rust library
code rather than spawning another WT executable.

Install extra names only where an external invocation contract requires them.
Those names are symlinks to the runtime's `wt` executable, which dispatches by
its invoked name:

- `git-remote-wt-agent` for Git's remote-helper discovery; and
- `codex` at the two supported wrapper locations for Codex prelaunch behavior.

Systemd and WT-owned callers use `wt` with an explicit subcommand and do not
receive compatibility aliases. Do not retain the current executable names as
general compatibility shims; this is an installation contract change and is
deployed as one coherent server and golden-image generation.

The standalone Git proxy and its installer remain separate release
executables. They form an independently installable product with different
configuration and credentials, as established by ADR 0016. Development-only
and test executables are outside this decision.

## Consequences

- A normal WT installation has one host program, one guest program in each
  image, and only protocol-required symlink entrypoints.
- One command tree makes installed capabilities discoverable and gives version
  and diagnostics commands a single release identity.
- Build and installer code still produces two artifacts because static guest
  portability and host libvirt linkage are incompatible requirements.
- Updating either artifact requires validating all of its command modes.
  Internal module boundaries and targeted tests remain even though executable
  packaging is consolidated.
- Scripts, service units, image contracts, and documentation must migrate
  atomically; old worlds continue to use the executable embedded in their
  original image generation.
