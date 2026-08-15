# wt-cli

The `wt` client.

## Owns

- Client configuration and selection of one local or OpenSSH context.
- The schema-versioned terminal message loop.
- Narrow workstation effects for Git identity, public keys, managed SSH,
  OpenSSH process replacement, and VS Code launch.

The client forwards server command arguments without parsing them. It does not
own command behavior, query multiple servers, run libvirt, Docker, or
provisioning.

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

User-visible behavior: [Worlds](../../docs/worlds/README.md). Transport and SSH
generation: [Client and SSH](../../docs/guides/client.md).
