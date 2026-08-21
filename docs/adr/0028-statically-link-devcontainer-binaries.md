# ADR 0028: Statically link installed WT binaries

- Status: Accepted
- Date: 2026-08-14

## Context

`wt-server-installer` builds WT on the Ubuntu server. World setup copies
`wt-tools` and `git-remote-wt-agent` into the guest and bind-mounts them
into the primary devcontainer.

The server and guest are WT-controlled Ubuntu 24.04 systems, but the repository
controls the devcontainer userland. A binary dynamically linked on the server
therefore inherits a glibc version requirement that the devcontainer may not
meet. In practice, `wt-tools` failed before it could connect to the relay because
it required GLIBC 2.39 and the devcontainer provided an older version.

WT must not make the server's libc version part of the devcontainer contract.

## Decision

Build every installed WT executable for `x86_64-unknown-linux-musl` except
`wt-server`. This includes the CLI, gateway, relay, Git helpers, and guest app
helpers. Build the installer executable for musl too. Keep `wt-agent-tool-gateway-hint`
as a POSIX shell asset.

`wt-server` remains a native GNU binary because it uses libvirt's supported C
ABI. The server runs only on the controlled Ubuntu 24.04 host where setup
installs that ABI. This is the only installed-executable exception to the musl
rule.

The installed musl artifacts must have no dynamic program interpreter or GLIBC
symbol-version requirement. Installation fails rather than falling back to a
dynamically linked devcontainer binary.

`scripts/install-server` installs the musl linker toolchain and the Rust musl
target. `wt-server-installer` rejects any designated static artifact with a dynamic
program interpreter or GLIBC symbol requirement before installing it.

This changes only executable packaging. The relay socket, transport protocol,
commands, and authorization model remain unchanged.

## Consequences

- `wt-tools` and normal Git operations through `git-remote-wt-agent` do not depend on
  the devcontainer's libc implementation or glibc version.
- Server installation produces one native server artifact and static musl
  artifacts for every other installed WT executable.
- The musl target matches WT's current amd64 platform requirement; this decision
  does not add another architecture.

## Alternatives

Build on an older glibc baseline. Rejected because it still makes an implicit
glibc floor part of the devcontainer contract and does not support musl-only
images.

Statically link `wt-server`. Rejected because that would replace or privately
rebuild libvirt's supported host ABI solely for packaging uniformity.
