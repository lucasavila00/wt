# wt-server-installer

Ubuntu 24.04 amd64 server installer and world-image builder.

Its runner, secure file installation, path handling, and SSH credential logic
come from `wt-installer-support`, which is also used by the standalone Git proxy setup.

```text
wt-server-installer validate --config PATH
wt-server-installer install --config PATH
wt-server-installer image build --config PATH
wt-server-installer image rebuild --config PATH
```

## Owns

- Install-input validation.
- Ubuntu, KVM, libvirt, directory, and permission setup.
- Strict `/etc/wt/server.toml` and `/etc/wt/capacity.toml` materialization and
  drift checks.
- Host-world image build, provenance, and verification.
- A verified development-tools image cache used only by opted-in golden-image builds.
- `wt` and `wt-server` binary installation.
- `wt-server.service` installation and startup under the installing user.

## Executable compatibility

`scripts/install-server` builds the setup tool and `wt-server` for the Ubuntu
GNU target because both link the host's supported libvirt ABI. Every other
installed WT executable is built as a static `x86_64-unknown-linux-musl`
binary. Setup verifies those installed artifacts have no ELF interpreter or
GLIBC symbol requirement. This covers the CLI, agent tool gateway and relay,
and Git helpers.

`PATH` is the install input. It is not the runtime config. Setup accepts matching
installed state and fails on drift or partial state.

Membership in `libvirt` grants control of the host hypervisor. Limit it to
trusted server users.
