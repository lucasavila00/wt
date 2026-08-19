# wt-server-setup

Ubuntu 24.04 amd64 server installer and world-image builder.

Its runner, secure file installation, path handling, and SSH credential logic
come from `wt-setup-core`, which is also used by the standalone Git proxy setup.

```text
wt-server-setup validate --config PATH
wt-server-setup install --config PATH
wt-server-setup image build --config PATH
wt-server-setup image rebuild --config PATH
```

## Owns

- Install-input validation.
- Ubuntu, KVM, libvirt, directory, and permission setup.
- Strict `/etc/wt/server.toml` and `/etc/wt/capacity.toml` materialization and
  drift checks.
- Registry-cache installation and verification.
- Devcontainer and host image build, provenance, and verification.
- `wt` and `wt-server` binary installation.
- `wt-server.service` installation and startup under the installing user.

## Executable compatibility

`scripts/install-server` builds the setup tool and every installed WT
executable except `wt-server` as static `x86_64-unknown-linux-musl` binaries.
Setup verifies installed artifacts have no ELF interpreter or GLIBC symbol
requirement. This covers the CLI, agent Git gateway and relay, Git helpers, and
guest app helpers. `wt-server` is the deliberate exception: it is built for the
Ubuntu GNU target because it links the host's supported libvirt ABI.

`PATH` is the install input. It is not the runtime config. Setup accepts matching
installed state and fails on drift or partial state.

Membership in `libvirt` grants control of the host hypervisor. Limit it to
trusted server users.

Usage: [Development and setup](../../DEVELOPMENT.md). Config samples:
[`examples/server-config/`](../../examples/server-config/).
