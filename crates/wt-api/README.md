# wt-api

Versioned control-plane types shared by `wt` and `wt-server`.

## Owns

- Protocol version 1 requests and responses.
- World state, SSH inventory, and error payloads.
- Instance name and SSH Git source validation.
- Passphrase redaction.

This crate performs no I/O and owns no transport or server configuration.

Protocol flow: [How WT works](../../docs/how/README.md#control-plane).
