# wt-api

Shared **control-plane** request/response types (JSON) for CLI and site servers.

Carried as **JSON over an SSH remote command** to a helper on the `wt-local` host (not a public HTTP API).

## Role

- Instance payloads (name, owner, source, status, **guest** SSH endpoint, errors)  
- Status and error enums (serde)  
- No I/O, libvirt, or transport  
- **Client context kinds** (e.g. `bare_metal_ssh`) may live in `wt-cli` config types rather than the wire API—plane does not need the client’s kubeconfig shape

CLI behavior: [docs/arch/cli.md](../../docs/arch/cli.md).

## Used by

| Crate | |
|-------|--|
| [`wt-cli`](../wt-cli/) | Client decoding (binary `wt`) |
| [`wt-local`](../wt-local/) | Server encoding |

Future multi-node binaries use the same control-plane types where they expose that API.

## Status

Library skeleton only; types not defined yet.
