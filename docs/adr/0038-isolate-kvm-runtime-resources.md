# ADR 0038: Make the agent Git endpoint configurable

- Status: Accepted
- Date: 2026-08-16

## Context

The installed gateway and KVM tests used the same fixed vsock port. A test could
not run beside production, and a single test-only port would still prevent
concurrent test runs.

## Decision

Treat the gateway vsock port as runtime configuration. Production config and
service units set `18017`. `WT_AGENT_GIT_VSOCK_PORT` may override it for an
unmanaged server or gateway process. Provision every world relay with the
server's resolved port.

Each KVM harness chooses a unique port through that environment variable. Its
Unix sockets and state remain under its existing temporary directory.

## Consequences

- Production and KVM gateways can run together.
- Independent KVM runs do not share a gateway endpoint.
- A server and its gateway must resolve the same override.
