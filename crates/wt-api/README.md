# wt-api

Shared **control-plane** request/response types (JSON) for CLI and site servers.

Carried **over SSH** to `wt-local` in the current design (not a public internet API contract by default).

## Role

- Instance payloads (name, owner, source, status, **guest** SSH endpoint, errors)  
- Status and error enums (serde)  
- No I/O, libvirt, or transport  

CLI behavior: [docs/arch/cli.md](../../docs/arch/cli.md).

## Used by

| Crate | |
|-------|--|
| [`wt-cli`](../wt-cli/) | Client decoding (binary `wt`) |
| [`wt-local`](../wt-local/) | Server encoding |

Future multi-node binaries use the same control-plane types where they expose that API.

## Status

Library skeleton only; types not defined yet.
