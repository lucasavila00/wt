# wt-cli

The `wt` client.

## Owns

- Local and OpenSSH server contexts.
- World naming and cross-context resolution.
- `new`, `logs`, `ls`, `code`, `rm`, and `sync`.
- Managed OpenSSH config and known hosts.
- Interactive Git-key passphrase input.

The client does not run libvirt, Docker, or provisioning.

## Install

From the workspace checkout:

```text
scripts/install-client
```

The script builds and replaces only the `wt` client in Cargo's binary directory.
It does not install or change the server.

## Run

```text
cargo run -p wt-cli -- --help
```

User-visible behavior: [What WT does](../../docs/what/README.md). Transport and
SSH generation: [How WT works](../../docs/how/cli.md).
