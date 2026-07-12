# wt-cli

The `wt` client.

## Owns

- Local and OpenSSH server contexts.
- World naming and cross-context resolution.
- `new`, `logs`, `ls`, `rm`, and `sync`.
- Managed OpenSSH config and known hosts.
- Interactive Git-key passphrase input.

The client does not run libvirt, Docker, or provisioning.

## Run

```text
cargo run -p wt-cli -- --help
```

Configuration, command behavior, and SSH aliases:
[CLI and SSH](../../docs/arch/cli.md).
