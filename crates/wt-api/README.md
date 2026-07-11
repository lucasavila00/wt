# wt-api

Shared **control-plane** request/response types for the CLI and site helper.

Carried as protocol version 1 JSON over stdio to `wt-local`, either locally or
through OpenSSH.

## Role

- Instance payloads (name, owner, status, guest IP, errors)
- Status and error enums (serde)  
- No I/O, libvirt, or transport  
- No transport or site configuration

CLI behavior: [docs/arch/cli.md](../../docs/arch/cli.md).

## Used by

| Crate | |
|-------|--|
| [`wt-cli`](../wt-cli/) | Client decoding (binary `wt`) |
| [`wt-local`](../wt-local/) | Site-helper decoding and encoding |
