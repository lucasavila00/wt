# wt-control-protocol

Versioned control-plane types shared by `wt` and `wt-server`.

## Owns

- Protocol version 3 requests and responses.
- World state, SSH inventory, Codex session observations, and error payloads.
- Instance name and SSH Git source validation.

This crate performs no I/O and owns no transport or server configuration.

Protocol flow: [Architecture](../../docs/internals/architecture.md).
