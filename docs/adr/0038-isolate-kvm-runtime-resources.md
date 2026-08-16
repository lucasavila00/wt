# ADR 0038: Isolate KVM test runtime resources

- Status: Accepted
- Date: 2026-08-16

## Context

KVM tests shared the installed gateway's fixed vsock port and golden images.
Tests could not run beside production safely, and one test namespace would
still prevent concurrent runs.

## Decision

Treat the gateway vsock port as runtime configuration. Production config and
service units set `18017`. `WT_AGENT_GIT_VSOCK_PORT` may override it for an
unmanaged server or gateway process. Provision every world relay with the
server's resolved port.

Each KVM harness chooses a unique port through that environment variable. Its
Unix sockets and state remain under its temporary directory.

Each harness also creates disposable qcow overlays on the installed golden
images. Tests update only their overlays with current branch assets. Production
images are read-only backing files and are never replaced by a test.

## Consequences

- Production and KVM gateways can run together.
- Independent KVM runs do not share gateway endpoints or writable images.
- A server and its gateway must resolve the same override.
- Test worlds must be deleted before their image overlays are removed.
