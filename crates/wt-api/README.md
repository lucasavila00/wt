# wt-api

Shared **control-plane API contract** library for the `wt` CLI and servers.

## Role

- Request/response types for the control-plane HTTP API  
- Status and error **enums** (serde-friendly), defined once  
- No I/O, no libvirt, no SSH — pure data + (later) serialization  

See [docs/arch/README.md](../../docs/arch/README.md).

## Consumers

| Crate | Use |
|-------|-----|
| [`wt`](../wt/) | Decode control-plane responses |
| [`wt-local`](../wt-local/) | Encode responses (v1 server) |
| future `wt-control-plane` / `wt-worker` | Same wire types where applicable |

## Non-goals

- Business logic / provision pipeline  
- Provider-specific types that never cross the wire (keep those in the agent)  

## Status

Topology only — no types implemented yet.
