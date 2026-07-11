# wt-api

Shared **API contract** library for the `wt` CLI and agents.

## Role

- Request/response types for the agent HTTP API  
- Status and error **enums** (serde-friendly), defined once  
- No I/O, no libvirt, no SSH — pure data + (later) serialization  

See [docs/arch/README.md](../../docs/arch/README.md).

## Consumers

| Crate | Use |
|-------|-----|
| [`wt`](../wt/) | Decode agent responses; share vocabulary with agent |
| [`wt-agent`](../wt-agent/) | Encode responses; same types as CLI |

## Non-goals

- Business logic / provision pipeline  
- Provider-specific types that never cross the wire (keep those in the agent)  

## Status

Topology only — no types implemented yet.
