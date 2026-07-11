# wt-api

Shared **control-plane** request/response types (JSON) for CLI and site servers.

Carried as JSON over stdio to the local `wt-local` helper in Era 1.

## Role

- Instance payloads (name, owner, source, status, guest IP, errors)  
- Status and error enums (serde)  
- No I/O, libvirt, or transport  
- Client context config lives in `wt-cli`, not the wire API

CLI behavior: [docs/arch/cli.md](../../docs/arch/cli.md).

## Used by

| Crate | |
|-------|--|
| [`wt-cli`](../wt-cli/) | Client decoding (binary `wt`) |
| [`wt-local`](../wt-local/) | Server encoding |

Future multi-node binaries use the same control-plane types where they expose that API.

## Status

Library skeleton only; types not defined yet.
