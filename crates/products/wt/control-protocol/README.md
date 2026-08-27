# wt-control-protocol

Versioned control-plane types shared by `wt` and `wt-server`.

## Owns

- Requests, progress events, and responses.
- World state, SSH inventory, pane observations, and error payloads.
- World name and SSH Git source validation.

This crate performs no I/O and owns no transport or server configuration.
