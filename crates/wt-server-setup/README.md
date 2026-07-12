# wt-server-setup

Ubuntu 24.04 amd64 server installer and golden-image builder.

```text
wt-server-setup validate --config PATH
wt-server-setup install --config PATH
wt-server-setup image build --config PATH
wt-server-setup image rebuild --config PATH
```

## Owns

- Install-input validation.
- Ubuntu, KVM, libvirt, directory, and permission setup.
- Strict `/etc/wt/server.toml` materialization and drift checks.
- Registry-cache installation and verification.
- Golden-image build, provenance, and verification.
- `wt` and `wt-server` binary installation.

`PATH` is the install input. It is not the runtime config. Setup accepts matching
installed state and fails on drift or partial state.

Membership in `libvirt` grants control of the host hypervisor. Limit it to
trusted server users.

Usage: [Getting started](../../GETTING-STARTED.md). Config samples:
[`examples/server-config/`](../../examples/server-config/).
