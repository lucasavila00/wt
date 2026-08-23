# wt-server-installer

Ubuntu 24.04 amd64 server installer and world-image builder.

Its runner, secure file installation, path handling, and SSH credential logic
come from `wt-installer-support`, which is also used by the standalone Git proxy setup.

```text
wts validate --config PATH
wts install --config PATH
wts image build --config PATH
wts image rebuild --config PATH
```

## Owns

- Install-input validation.
- Ubuntu, KVM, libvirt, directory, and permission setup.
- Strict `/etc/wt/server.toml` and `/etc/wt/capacity.toml` materialization and
  drift checks.
- Guest image build, provenance, and verification.
- A verified development-tools image cache used only by opted-in golden-image builds.
- `wts` binary installation.
- `wts.service` installation and startup under the installing user.

## Executable compatibility

`scripts/install-server` builds `wts` for the Ubuntu GNU target because it links
the server's supported libvirt ABI. It builds `wtg` as a static
`x86_64-unknown-linux-musl` guest binary and verifies that artifact has no ELF
interpreter or GLIBC symbol requirement.

`PATH` is the install input. It is not the runtime config. Setup accepts matching
installed state and fails on drift or partial state.

Membership in `libvirt` grants control of the host hypervisor. Limit it to
trusted server users.
