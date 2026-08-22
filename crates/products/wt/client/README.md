# wt-client

The `wt` client.

## Owns

- Local and OpenSSH server contexts.
- World naming and cross-context resolution.
- `new`, `ls`, `start`, `stop`, `ssh`, `shell`, `rm`, and `sync`.
- Managed OpenSSH config and known hosts.
- Managed SSH aliases with pinned host identities.

The client does not run libvirt or provisioning.

## Install

From the workspace checkout:

```text
scripts/install-client
```

The script builds and replaces only the `wt` client in Cargo's binary directory.
It does not install or change the server.

## Run

```text
scripts/cargo run -p wt-client -- --help
```

User-visible behavior, transport, and SSH generation:
[Client and SSH](../../../../docs/guides/client.md).
