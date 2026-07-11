# wt-api

Shared **control-plane** HTTP/JSON types for `wt` and site servers.

## Role

- Request/response types  
- Status and error enums (serde)  
- No I/O, libvirt, or SSH  

## Used by

| Crate | |
|-------|--|
| [`wt`](../wt/) | Client decoding |
| [`wt-local`](../wt-local/) | Server encoding |

Future multi-node binaries use the same control-plane types where they expose that API.

## Status

Library skeleton only; types not defined yet.
