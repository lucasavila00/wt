# ADR 0028: Statically link WT binaries that run in devcontainers

- Status: Accepted
- Date: 2026-08-14
- Amends: [ADR 0017](0017-integrate-agent-git-gateway.md)

## Context

`wt-server-setup` builds WT on the Ubuntu server. World setup copies `ag-git`
and `git-remote-ag` into the guest and bind-mounts them into the primary
devcontainer.

The server and guest are WT-controlled Ubuntu 24.04 systems, but the repository
controls the devcontainer userland. A binary dynamically linked on the server
therefore inherits a glibc version requirement that the devcontainer may not
meet. In practice, `ag-git` failed before it could connect to the relay because
it required GLIBC 2.39 and the devcontainer provided an older version.

WT must not make the server's libc version part of the devcontainer contract.

## Decision

Build the Rust executables that WT runs inside the devcontainer—`ag-git` and
`git-remote-ag`—for `x86_64-unknown-linux-musl` and statically link them. Use
those artifacts both for the guest-side Git flow and for the devcontainer bind
mounts.

Continue to build the gateway, relay, client, server, and guest helpers for the
native GNU target. They run only on WT-controlled systems and do not cross the
devcontainer boundary. Keep `wt-agent-git-hint` as a POSIX shell asset.

The installed musl artifacts must have no dynamic program interpreter or GLIBC
symbol-version requirement. Installation fails rather than falling back to a
dynamically linked devcontainer binary.

`scripts/install-server` installs the musl linker toolchain and the Rust musl
target. `wt-server-setup` inspects both artifacts before installing them.

This changes only executable packaging. The relay socket, transport protocol,
commands, and authorization model remain unchanged.

## Consequences

- `ag-git` and normal Git operations through `git-remote-ag` do not depend on
  the devcontainer's libc implementation or glibc version.
- Server installation needs the Rust musl target and its linker toolchain and
  produces native and musl release artifacts.
- WT does not take responsibility for statically linking binaries that remain
  within its controlled server and guest environments.
- The musl target matches WT's current amd64 platform requirement; this decision
  does not add another architecture.
- Existing worlds retain their copied client binaries and must be recreated to
  receive the statically linked artifacts.

## Alternatives

Build on an older glibc baseline. Rejected because it still makes an implicit
glibc floor part of the devcontainer contract and does not support musl-only
images.

Statically link every WT executable. Rejected because only the two executables
that cross into an uncontrolled userland need this compatibility boundary.
